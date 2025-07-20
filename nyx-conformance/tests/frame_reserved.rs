use nyx_stream::parse_header_ext;

#[test]
fn header_reserved_type_error() {
    // frame_type=3 reserved, flags zero, length 0
    let bytes = [0xC0u8, 0x00u8, 0x00u8, 0x00u8];
    let (_, hdr) = parse_header_ext(&bytes).expect("parse");
    // Reserved type should be 3
    assert_eq!(hdr.hdr.frame_type, 3);
} 