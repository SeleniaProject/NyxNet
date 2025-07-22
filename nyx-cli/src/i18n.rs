//! Simple Fluent-based i18n helper for Nyx CLI.
#![forbid(unsafe_code)]

use fluent_bundle::{FluentBundle, FluentResource, FluentArgs, FluentValue};
use unic_langid::LanguageIdentifier;
use once_cell::sync::Lazy;
use std::borrow::Cow;
use std::str::FromStr;

static EN_RES: &str = include_str!("../i18n/en.ftl");
static JA_RES: &str = include_str!("../i18n/ja.ftl");
static ZH_RES: &str = include_str!("../i18n/zh.ftl");

static BUNDLE_EN: Lazy<FluentBundle<FluentResource>> = Lazy::new(|| load("en-US", EN_RES));
static BUNDLE_JA: Lazy<FluentBundle<FluentResource>> = Lazy::new(|| load("ja", JA_RES));
static BUNDLE_ZH: Lazy<FluentBundle<FluentResource>> = Lazy::new(|| load("zh-CN", ZH_RES));

fn load(lang: &str, res: &str) -> FluentBundle<FluentResource> {
    let langid = LanguageIdentifier::from_str(lang).unwrap();
    let mut bundle = FluentBundle::new(vec![langid]);
    let res = FluentResource::try_new(res.to_string()).expect("parse ftl");
    bundle.add_resource(res).expect("add ftl");
    bundle
}

/// Resolve message for given key and optional args according to current locale.
pub fn tr(key: &str, args: Option<&FluentArgs>) -> String {
    let locale = std::env::var("NYX_LANG").or_else(|_| std::env::var("LANG")).unwrap_or_else(|_| "en".to_string());
    let bundle = if locale.starts_with("ja") { &*BUNDLE_JA } else if locale.starts_with("zh") { &*BUNDLE_ZH } else { &*BUNDLE_EN };
    let msg = bundle.get_message(key).expect("message exists");
    let pattern = msg.value().expect("has value");
    let mut errors = vec![];
    let value = bundle
        .format_pattern(pattern, args.unwrap_or(&FluentArgs::default()), &mut errors);
    if !errors.is_empty() {
        eprintln!("[i18n] format errors: {:?}", errors);
    }
    value.into_owned()
}

/// Convenience to create args with single named param.
#[macro_export]
macro_rules! i18n_args {
    ($name:expr => $value:expr) => {{
        let mut a = fluent_bundle::FluentArgs::new();
        a.set($name, FluentValue::from($value));
        a
    }};
} 