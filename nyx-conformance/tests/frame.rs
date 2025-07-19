use nyx_stream::frame::{parse_header, FrameHeader};

#[test]
fn frame_header_parse() {
    // frame_type=1, flags=0x15, length=100
    let bytes = [0x55u8, 0x00u8, 0x64u8, 0x00u8];
    let (_, hdr) = parse_header(&bytes).expect("parse");
    assert_eq!(hdr.frame_type, 1);
    assert_eq!(hdr.flags, 0x15);
    assert_eq!(hdr.length, 100);
} 