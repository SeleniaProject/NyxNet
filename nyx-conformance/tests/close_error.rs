use nyx_stream::parse_close_frame;

#[test]
fn close_frame_parse_error() {
    // Close frame requires at least 3 bytes (code + len), feed 2 bytes.
    let data = [0x00u8, 0x01u8];
    assert!(parse_close_frame(&data).is_err());
} 