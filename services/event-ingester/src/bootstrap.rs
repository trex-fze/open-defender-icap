use crate::{config::IngestConfig, elastic::ElasticWriter};
use anyhow::Context;
use serde_json::Value;

const TEMPLATE_JSON: &str = include_str!("../../../deploy/elastic/index-template.json");
const ILM_JSON: &str = include_str!("../../../deploy/elastic/ilm-policy.json");

pub async fn ensure_assets(writer: &ElasticWriter, config: &IngestConfig) -> anyhow::Result<()> {
    if !config.apply_templates {
        return Ok(());
    }

    let mut template: Value = serde_json::from_str(TEMPLATE_JSON)?;
    template["index_patterns"] =
        Value::Array(vec![Value::String(config.elastic_index_pattern.clone())]);
    template["template"]["settings"]["index.lifecycle.name"] =
        Value::String(config.elastic_ilm_name.clone());

    let ilm: Value = serde_json::from_str(ILM_JSON)?;

    writer
        .put_index_template(&config.elastic_template_name, &template)
        .await
        .context("failed to apply index template")?;

    writer
        .put_ilm_policy(&config.elastic_ilm_name, &ilm)
        .await
        .context("failed to apply ILM policy")?;

    Ok(())
}
