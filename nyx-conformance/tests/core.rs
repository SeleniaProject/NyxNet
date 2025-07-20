use nyx_core::config::NyxConfig;
use nyx_core::i18n::{tr, Lang};
use std::{fs, path::PathBuf};

#[test]
fn nyx_config_parse_defaults() {
    // Prepare minimal TOML (only node_id provided) in temp directory.
    let dir = std::env::temp_dir();
    let mut path = PathBuf::from(dir);
    path.push("nyx_cfg_test.toml");
    fs::write(&path, "node_id = \"abcd1234\"").unwrap();

    let cfg = NyxConfig::from_file(&path).expect("parse");
    assert_eq!(cfg.node_id.as_deref(), Some("abcd1234"));
    // Defaults should be filled
    assert_eq!(cfg.listen_port, 43300);
    assert_eq!(cfg.log_level.as_deref(), Some("info"));

    fs::remove_file(path).ok();
}

#[test]
fn i18n_japanese_translation_contains_port() {
    let msg = tr(Lang::Ja, "init-success", None);
    // Japanese string should include kanji and the word "ポート"
    assert!(msg.contains("ポート"));
} 