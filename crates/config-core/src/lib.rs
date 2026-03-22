use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::{env, fs, path::Path};

pub fn load_config<T>(path: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let file = if Path::new(path).exists() {
        Some(fs::read_to_string(path).with_context(|| format!("failed to read config {path}"))?)
    } else {
        None
    };

    if let Some(contents) = file {
        let cfg = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse config {path} as JSON"))?;
        Ok(cfg)
    } else {
        let env_json = env::var("OD_CONFIG_JSON").unwrap_or_else(|_| "{}".to_string());
        let cfg = serde_json::from_str(&env_json).context("failed to parse OD_CONFIG_JSON")?;
        Ok(cfg)
    }
}
