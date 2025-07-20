use nyx_stream::parse_header_ext;

#[test]
fn parse_header_ext_short_buffer() {
    // buffer too short (<4 bytes)
    let data = [0u8; 2];
    assert!(parse_header_ext(&data).is_err());
} 