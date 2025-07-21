#![cfg(feature = "plugin")]
use nyx_stream::PluginHeader;

#[test]
fn plugin_header_encode_decode() {
    let hdr = PluginHeader { id: 42, flags: 0xA5, data: b"hello" };
    let encoded = hdr.encode();
    let decoded = PluginHeader::decode(&encoded).expect("decode");
    assert_eq!(hdr, decoded);
}

#[test]
fn plugin_header_decode_error() {
    let bytes = [0xFFu8; 3];
    assert!(PluginHeader::decode(&bytes).is_err());
} 