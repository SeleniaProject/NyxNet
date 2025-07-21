#![cfg(feature = "plugin")]
use nyx_stream::{PluginHeader};

#[test]
fn plugin_flags_roundtrip() {
    let hdr = PluginHeader { id: 1, flags: 0xFF, data: b"" };
    let enc = hdr.encode();
    let dec = PluginHeader::decode(&enc).unwrap();
    assert_eq!(hdr, dec);
} 