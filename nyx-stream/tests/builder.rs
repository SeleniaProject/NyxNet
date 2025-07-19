use nyx_stream::{FrameHeader, build_header, parse_header};

#[test]
fn header_roundtrip() {
    let hdr = FrameHeader { frame_type: 1, flags: 0x2A, length: 0x30 };
    let bytes = build_header(hdr);
    let (_, parsed) = parse_header(&bytes).unwrap();
    assert_eq!(hdr.frame_type, parsed.frame_type);
    assert_eq!(hdr.flags, parsed.flags);
    assert_eq!(hdr.length, parsed.length);
} 