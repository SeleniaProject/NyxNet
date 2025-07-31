use nyx_stream::{FrameHeader, build_header, parse_header, parse_header_ext};
use nyx_stream::builder::build_header_ext;

#[test]
fn header_roundtrip() {
    let hdr = FrameHeader { frame_type: 1, flags: 0x2A, length: 0x30 };
    let bytes = build_header(hdr);
    let (_, parsed) = parse_header(&bytes).unwrap();
    assert_eq!(hdr.frame_type, parsed.frame_type);
    assert_eq!(hdr.flags, parsed.flags);
    assert_eq!(hdr.length, parsed.length);
}

#[test]
fn header_with_path_id_roundtrip() {
    let hdr = FrameHeader { frame_type: 2, flags: 0x05, length: 100 };
    let bytes = build_header_ext(hdr, Some(7));
    let (_, parsed) = parse_header_ext(&bytes).unwrap();
    assert_eq!(parsed.hdr.frame_type, 2);
    assert_eq!(parsed.hdr.length, 100);
    assert_eq!(parsed.path_id, Some(7));
} 