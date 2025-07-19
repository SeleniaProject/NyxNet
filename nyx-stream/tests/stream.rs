use nyx_stream::{StreamFrame, build_stream_frame, parse_stream_frame};

#[test]
fn stream_frame_roundtrip() {
    let data = b"hello";
    let frame = StreamFrame { stream_id: 1, offset: 0, fin: false, data };
    let bytes = build_stream_frame(&frame);
    let (_, parsed) = parse_stream_frame(&bytes).unwrap();
    assert_eq!(frame.stream_id, parsed.stream_id);
    assert_eq!(frame.offset, parsed.offset);
    assert_eq!(frame.fin, parsed.fin);
    assert_eq!(frame.data, parsed.data);
} 