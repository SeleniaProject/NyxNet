#![forbid(unsafe_code)]

//! Plugin Frame (Type 0x50–0x5F) encoder/decoder.
//! Payload is a CBOR header `{id:u32, flags:u8, data:bytes}` followed by raw data.

use serde::{Serialize, Deserialize};
use schemars::JsonSchema;
use jsonschema::{JSONSchema, Draft};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct PluginHeader<'a> {
    pub id: u32,
    pub flags: u8,
    #[serde(with = "serde_bytes")]
    pub data: &'a [u8],
}

impl<'a> PluginHeader<'a> {
    pub fn encode(&self) -> Vec<u8> {
        serde_cbor::to_vec(self).expect("encode cbor")
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, serde_cbor::Error> {
        serde_cbor::from_slice(bytes)
    }

    /// Validate raw CBOR bytes against the auto‐generated JSON Schema and
    /// return decoded header on success.
    pub fn validate(bytes: &'a [u8]) -> Result<Self, String> {
        // Decode first (serde ensures structural CBOR correctness).
        let hdr: Self = serde_cbor::from_slice(bytes).map_err(|e| e.to_string())?;
        // Convert header to JSON for schema validation.
        let json_val = serde_json::to_value(&hdr).map_err(|e| e.to_string())?;
        let schema_json = schemars::schema_for!(PluginHeader<'static>);
        let compiled = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema_json.schema)
            .map_err(|e| e.to_string())?;
        if let Err(errors) = compiled.validate(&json_val) {
            let msg = errors
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(msg)
        } else {
            Ok(hdr)
        }
    }
} 