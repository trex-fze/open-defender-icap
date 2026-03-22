use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct IcapConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_preview_size")]
    pub preview_size: usize,
    pub redis_url: Option<String>,
    pub policy_endpoint: Option<String>,
    #[serde(default = "default_metrics_host")]
    pub metrics_host: String,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
}

pub fn load() -> anyhow::Result<IcapConfig> {
    let cfg = config_core::load_config::<IcapConfig>("config/icap.json")?;
    Ok(cfg)
}

const fn default_preview_size() -> usize {
    4096
}

fn default_metrics_host() -> String {
    "0.0.0.0".to_string()
}

const fn default_metrics_port() -> u16 {
    19005
}
