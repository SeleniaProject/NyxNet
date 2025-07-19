use nyx_stream::{parse_header, FrameHeader};

#[test]
fn parse_basic_header() {
    // frame_type=2 (10), flags=0x15, length=0x20
    let header_bytes = [0b10010101u8, 0x00, 0x20, 0x00];
    let (_, hdr) = parse_header(&header_bytes).unwrap();
    assert_eq!(hdr.frame_type, 2);
    assert_eq!(hdr.flags, 0x15);
    assert_eq!(hdr.length, 0x20);
} 