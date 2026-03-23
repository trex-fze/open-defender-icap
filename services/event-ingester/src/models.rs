use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FilebeatEnvelope {
    Events { events: Vec<Value> },
    Array(Vec<Value>),
    Single(Value),
}

impl FilebeatEnvelope {
    pub fn into_events(self) -> Vec<Value> {
        match self {
            FilebeatEnvelope::Events { events } => events,
            FilebeatEnvelope::Array(values) => values,
            FilebeatEnvelope::Single(value) => vec![value],
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

impl HealthResponse {
    pub fn ok() -> Self {
        Self { status: "ok" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_converts_variants() {
        let events = FilebeatEnvelope::Events {
            events: vec![Value::String("a".into()), Value::String("b".into())],
        };
        assert_eq!(events.into_events().len(), 2);

        let array = FilebeatEnvelope::Array(vec![Value::Number(1.into())]);
        assert_eq!(array.into_events().len(), 1);

        let single = FilebeatEnvelope::Single(Value::Bool(true));
        assert_eq!(single.into_events().len(), 1);
    }
}
