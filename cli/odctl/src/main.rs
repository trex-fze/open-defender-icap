use anyhow::{anyhow, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());

    match command.as_str() {
        "health" => run_health().await?,
        "smoke" => run_smoke(args).await?,
        "help" | "--help" | "-h" => print_help(),
        other => return Err(anyhow!("unknown command: {other}")),
    }

    Ok(())
}

fn print_help() {
    println!("odctl commands:\n  health    Run health checks\n  smoke     Run smoke tests\n");
}

async fn run_health() -> Result<()> {
    info!("odctl health placeholder");
    Ok(())
}

async fn run_smoke(mut args: impl Iterator<Item = String>) -> Result<()> {
    let target = args.next().unwrap_or_else(|| "127.0.0.1:1344".to_string());
    let request = "REQMOD icap://icap.service/req ICAP/1.0\r\nHost: icap.service\r\nX-Trace-Id: odctl-smoke\r\nEncapsulated: req-body=0, null-body=0\r\n\r\nGET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n";

    let mut stream = TcpStream::connect(&target).await?;
    stream.write_all(request.as_bytes()).await?;
    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);
    if response.starts_with("ICAP/1.0 204") || response.starts_with("ICAP/1.0 200") {
        println!("Smoke test succeeded against {target}: {response}");
        Ok(())
    } else {
        Err(anyhow!("unexpected ICAP response: {response}"))
    }
}
