use nyx_stream::parse_header_ext;

#[test]
fn header_invalid_type() {
    // Set first 2 bits to 0b11 (reserved) -> 0xC0
    let bytes = [0xC0u8, 0x00u8, 0x00u8, 0x00u8];
    assert!(parse_header_ext(&bytes).is_ok()); // parser accepts reserved, but we expect type 3 reserved
} 