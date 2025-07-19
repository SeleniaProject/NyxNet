use nyx_stream::{StreamFrame, build_stream_frame, parse_stream_frame};

#[test]
fn stream_frame_roundtrip() {
    let payload = b"NyxTest";
    let frame = StreamFrame { stream_id: 7, offset: 0, fin: false, data: payload };
    let encoded = build_stream_frame(&frame);
    let (_, decoded) = parse_stream_frame(&encoded).expect("parse");
    assert_eq!(frame, decoded);
} 