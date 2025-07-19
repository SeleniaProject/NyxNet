#![forbid(unsafe_code)]

//! Plugin Frame (Type 0x50–0x5F) encoder/decoder.
//! Payload is a CBOR header `{id:u32, flags:u8, data:bytes}` followed by raw data.

use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
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
} 