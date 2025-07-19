use nyx_stream::{LocalizedStringFrame, build_localized_string_frame, parse_localized_string_frame};

#[test]
fn localized_string_roundtrip_en() {
    let frame = LocalizedStringFrame { lang_tag: "en", text: "Hello" };
    let encoded = build_localized_string_frame(&frame);
    let (_, decoded) = parse_localized_string_frame(&encoded).expect("parse");
    assert_eq!(frame, decoded);
}

#[test]
fn localized_string_roundtrip_ja() {
    let frame = LocalizedStringFrame { lang_tag: "ja", text: "こんにちは" };
    let encoded = build_localized_string_frame(&frame);
    let (_, decoded) = parse_localized_string_frame(&encoded).expect("parse");
    assert_eq!(frame, decoded);
} 