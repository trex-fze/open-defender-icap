use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyConfig {
    pub api_host: String,
    pub api_port: u16,
    pub policy_file: String,
}

pub fn load() -> anyhow::Result<PolicyConfig> {
    let cfg = config_core::load_config::<PolicyConfig>("config/policy-engine.json")?;
    Ok(cfg)
}
