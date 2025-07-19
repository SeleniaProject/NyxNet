use nyx_stream::{build_close_frame, parse_close_frame, build_path_challenge_frame, parse_path_challenge_frame, build_path_response_frame, parse_path_response_frame, Setting, build_settings_frame, parse_settings_frame};

#[test]
fn close_frame_roundtrip() {
    let reason = b"normal";
    let code = 0x0000u16;
    let encoded = build_close_frame(code, reason);
    let (_, decoded) = parse_close_frame(&encoded).expect("parse");
    assert_eq!(decoded.code, code);
    assert_eq!(decoded.reason, reason);
}

#[test]
fn path_challenge_roundtrip() {
    let token: [u8;16] = [0xAB; 16];
    let encoded = build_path_challenge_frame(&token);
    let (_, decoded) = parse_path_challenge_frame(&encoded).expect("parse");
    assert_eq!(decoded.token, token);
}

#[test]
fn path_response_roundtrip() {
    let token: [u8;16] = [0xCD; 16];
    let encoded = build_path_response_frame(&token);
    let (_, decoded) = parse_path_response_frame(&encoded).expect("parse");
    assert_eq!(decoded.token, token);
}

#[test]
fn settings_frame_roundtrip() {
    let settings = vec![Setting { id: 0x0001, value: 100 }, Setting { id: 0x0002, value: 65535 }];
    let encoded = build_settings_frame(&settings);
    let (_, decoded) = parse_settings_frame(&encoded).expect("parse");
    assert_eq!(decoded.settings, settings);
} 