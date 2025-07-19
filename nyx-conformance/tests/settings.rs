use nyx_control::settings::validate_settings;
use nyx_stream::{StreamSettings, build_settings_frame, parse_settings_frame};

#[test]
fn valid_settings_json() {
    let json = br#"{"max_streams":128,"max_data":2048,"idle_timeout":15,"pq_supported":true}"#;
    let cfg = validate_settings(json).expect("valid");
    assert_eq!(cfg.max_streams, 128);
    assert!(cfg.pq_supported);
}

#[test]
fn invalid_settings_missing_field() {
    let json = br#"{"max_streams":128}"#;
    let err = validate_settings(json).unwrap_err();
    assert!(err.contains("schema error"));
}

#[test]
fn settings_roundtrip_defaults() {
    let s = StreamSettings::default();
    let frame = s.to_frame();
    let encoded = build_settings_frame(&frame.settings);
    let (_, decoded) = parse_settings_frame(&encoded).expect("parse");
    let mut applied = StreamSettings::default();
    applied.apply(&decoded);
    assert_eq!(applied, s);
}

#[test]
fn settings_custom() {
    let mut s = StreamSettings::default();
    s.max_streams = 512;
    s.max_data = 2_097_152;
    s.idle_timeout = 45;
    s.pq_supported = true;
    let frame = s.to_frame();
    let enc = build_settings_frame(&frame.settings);
    let (_, dec) = parse_settings_frame(&enc).unwrap();
    let mut applied = StreamSettings::default();
    applied.apply(&dec);
    assert_eq!(applied, s);
} 