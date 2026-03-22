use anyhow::Result;
use serde::Serialize;
use tracing::info;

#[derive(Clone)]
pub struct JobPublisher {
    client: redis::Client,
    stream: String,
}

#[derive(Debug, Serialize)]
pub struct ClassificationJob<'a> {
    pub normalized_key: &'a str,
    pub entity_level: &'a str,
    pub hostname: &'a str,
    pub full_url: &'a str,
    pub trace_id: &'a str,
}

impl JobPublisher {
    pub fn new(redis_url: &str, stream: String) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client, stream })
    }

    pub async fn publish(&self, job: &ClassificationJob<'_>) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let payload = serde_json::to_string(&job)?;
        let _: () = redis::cmd("XADD")
            .arg(&self.stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut conn)
            .await?;
        info!(target = "svc-icap", stream = %self.stream, key = job.normalized_key, "published classification job");
        Ok(())
    }
}
