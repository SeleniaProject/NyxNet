use nyx_stream::{FLAG_HAS_PATH_ID, parse_header_ext};

#[test]
fn header_with_path_id_parses_correctly() {
    // frame_type=2 (0b10), flags=0x25 (0x20 path flag + 0x05), length=50, path_id=9
    let bytes = [0xA5u8, 0x00u8, 0x32u8, 0x00u8, 0x09u8];
    let (_, parsed) = parse_header_ext(&bytes).expect("parse");
    assert_eq!(parsed.hdr.frame_type, 2);
    assert_eq!(parsed.hdr.flags & FLAG_HAS_PATH_ID, FLAG_HAS_PATH_ID);
    assert_eq!(parsed.hdr.length, 50);
    assert_eq!(parsed.path_id, Some(9));
}

#[test]
fn header_without_path_id_flag() {
    let bytes = [0x55u8, 0x00u8, 0x64u8, 0x00u8];
    let (_, parsed) = parse_header_ext(&bytes).expect("parse");
    assert!(parsed.path_id.is_none());
} 