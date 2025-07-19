use nyx_control::settings::{validate_settings, Settings};

#[test]
fn valid_settings_pass() {
    let json = br#"{"max_streams":128,"max_data":1048576,"idle_timeout":20,"pq_supported":true}"#;
    let s = validate_settings(json).expect("valid");
    assert_eq!(s.max_streams, 128);
}

#[test]
fn invalid_settings_fail() {
    let json = br#"{"max_streams":"bad"}"#;
    assert!(validate_settings(json).is_err());
} 