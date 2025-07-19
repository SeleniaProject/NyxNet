use nyx_stream::{PingFrame, build_ping_frame, parse_ping_frame, PongFrame, build_pong_frame, parse_pong_frame};

#[test]
fn ping_frame_roundtrip() {
    let frame = PingFrame { nonce: 0x1122334455667788 };
    let encoded = build_ping_frame(&frame);
    let (_, decoded) = parse_ping_frame(&encoded).expect("parse");
    assert_eq!(frame, decoded);
}

#[test]
fn pong_frame_roundtrip() {
    let frame = PongFrame { nonce: 0x8877665544332211 };
    let encoded = build_pong_frame(&frame);
    let (_, decoded) = parse_pong_frame(&encoded).expect("parse");
    assert_eq!(frame, decoded);
} 