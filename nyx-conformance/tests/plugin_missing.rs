use nyx_stream::PluginHeader;

#[test]
fn plugin_header_missing_field() {
    // Construct CBOR map missing 'data' field
    // {"id":1,"flags":0xFF}
    let cbor = [0xA2u8, 0x62, 0x69, 0x64, 0x01, 0x65, 0x66, 0x6C, 0x61, 0x67, 0x73, 0x18, 0xFF];
    assert!(PluginHeader::decode(&cbor).is_err());
} 