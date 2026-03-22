use anyhow::{anyhow, Result};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());

    match command.as_str() {
        "health" => run_health().await?,
        "smoke" => run_smoke().await?,
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

async fn run_smoke() -> Result<()> {
    info!("odctl smoke placeholder");
    Ok(())
}
