use nyx_stream::parse_settings_frame;

#[test]
fn settings_frame_too_short() {
    // Proper settings frame requires multiples of 6 bytes (id:u16,value:u32).
    // Provide 5 bytes to trigger error.
    let invalid = [0x00u8, 0x01u8, 0x00u8, 0x00u8, 0x00u8];
    assert!(parse_settings_frame(&invalid).is_err());
} 