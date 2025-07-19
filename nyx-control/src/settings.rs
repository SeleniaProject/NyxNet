//! SETTINGS frame schema and validation utilities.
//! According to Nyx Protocol v0.1 §16.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// SETTINGS payload structure.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Settings {
    /// Maximum concurrent streams allowed.
    #[serde(default = "default_max_streams", rename = "max_streams")]
    pub max_streams: u32,

    /// Maximum connection-wide data in bytes.
    #[serde(default = "default_max_data", rename = "max_data")]
    pub max_data: u32,

    /// Idle timeout in seconds.
    #[serde(default = "default_idle", rename = "idle_timeout")]
    pub idle_timeout: u16,

    /// Whether the peer supports PQ fallback.
    #[serde(default, rename = "pq_supported")]
    pub pq_supported: bool,
}

const fn default_max_streams() -> u32 { 256 }
const fn default_max_data() -> u32 { 1_048_576 }
const fn default_idle() -> u16 { 30 }

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_streams: default_max_streams(),
            max_data: default_max_data(),
            idle_timeout: default_idle(),
            pq_supported: false,
        }
    }
}

/// Validate JSON byte payload against schema and return parsed Settings.
pub fn validate_settings(json: &[u8]) -> Result<Settings, String> {
    let val: serde_json::Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let schema = schemars::schema_for!(Settings);
    let schema_value = serde_json::to_value(&schema.schema).unwrap();
    let compiled = jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .compile(&schema_value)
        .map_err(|e| e.to_string())?;
    compiled.validate(&val).map_err(|err_iter| {
        let joined = err_iter.map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
        format!("schema error: {}", joined)
    }).map(|_| serde_json::from_value(val).unwrap())
} 