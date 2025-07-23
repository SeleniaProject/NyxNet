//! Simple Fluent-based i18n helper for Nyx CLI.
#![forbid(unsafe_code)]

use fluent_bundle::{FluentBundle, FluentResource};
use std::sync::OnceLock;

static EN_RES: &str = include_str!("../i18n/en.ftl");
static JA_RES: &str = include_str!("../i18n/ja.ftl");
static ZH_RES: &str = include_str!("../i18n/zh.ftl");

fn load(locale: &str, content: &str) -> FluentBundle<FluentResource> {
    let resource = FluentResource::try_new(content.to_string())
        .expect("Failed to parse FTL string");
    
    let mut bundle = FluentBundle::new(vec![locale.parse().expect("Invalid locale")]);
    bundle.add_resource(resource)
        .expect("Failed to add resource");
    
    bundle
}

static BUNDLE_EN: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();
static BUNDLE_JA: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();
static BUNDLE_ZH: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

fn get_bundle(language: &str) -> &'static FluentBundle<FluentResource> {
    match language {
        "ja" => BUNDLE_JA.get_or_init(|| load("ja", JA_RES)),
        "zh" => BUNDLE_ZH.get_or_init(|| load("zh-CN", ZH_RES)),
        _ => BUNDLE_EN.get_or_init(|| load("en-US", EN_RES)),
    }
}

pub fn localize(
    language: &str,
    text_id: &str,
    args: Option<&fluent_bundle::FluentArgs>,
) -> Result<String, Box<dyn std::error::Error>> {
    let bundle = get_bundle(language);
    
    let msg = bundle.get_message(text_id)
        .ok_or_else(|| format!("Message '{}' not found", text_id))?;
    
    let pattern = msg.value()
        .ok_or_else(|| format!("Message '{}' has no value", text_id))?;
    
    let mut errors = Vec::new();
    let formatted = bundle
        .format_pattern(pattern, args, &mut errors);
    
    if !errors.is_empty() {
        eprintln!("Fluent formatting errors: {:?}", errors);
    }
    
    Ok(formatted.into_owned())
} 