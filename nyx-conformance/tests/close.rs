use nyx_stream::frame::parse_header;

#[test]
fn close_frame_header_parse() {
    // frame_type=3 (CLOSE), flags=0x04, length=0
    let bytes = [0xC4u8, 0x00, 0x00, 0x00];
    let (_, hdr) = parse_header(&bytes).expect("parse");
    assert_eq!(hdr.frame_type, 3);
    assert_eq!(hdr.flags, 0x04);
    assert_eq!(hdr.length, 0);
} 