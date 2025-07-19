use nyx_control::settings::{validate_settings, Settings};

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