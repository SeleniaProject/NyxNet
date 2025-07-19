use fluent_bundle::{FluentBundle, FluentResource, FluentArgs, FluentValue};
use unic_langid::LanguageIdentifier;
use once_cell::sync::Lazy;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy)]
pub enum Lang {
    En,
    Zh,
    Ja,
}

impl Lang {
    fn langid(self) -> &'static LanguageIdentifier {
        match self {
            Lang::En => &EN_ID,
            Lang::Zh => &ZH_ID,
            Lang::Ja => &JA_ID,
        }
    }
}

static EN_ID: Lazy<LanguageIdentifier> = Lazy::new(|| "en".parse().unwrap());
static ZH_ID: Lazy<LanguageIdentifier> = Lazy::new(|| "zh".parse().unwrap());
static JA_ID: Lazy<LanguageIdentifier> = Lazy::new(|| "ja".parse().unwrap());

static EN_BUNDLE: Lazy<FluentBundle<&'static FluentResource>> = Lazy::new(|| make_bundle(*EN_ID, EN_FTL));
static ZH_BUNDLE: Lazy<FluentBundle<&'static FluentResource>> = Lazy::new(|| make_bundle(*ZH_ID, ZH_FTL));
static JA_BUNDLE: Lazy<FluentBundle<&'static FluentResource>> = Lazy::new(|| make_bundle(*JA_ID, JA_FTL));

const EN_FTL: &str = r#"init-success = Nyx daemon started (port { $port })
config-reload = Configuration reloaded
"#;

const JA_FTL: &str = r#"init-success = Nyx デーモン起動完了 (ポート { $port })
config-reload = 設定をリロードしました
"#;

const ZH_FTL: &str = r#"init-success = Nyx 守护进程已启动（端口 { $port }）
config-reload = 配置已重新加载
"#;

fn make_bundle<'a>(lang: LanguageIdentifier, src: &'static str) -> FluentBundle<&'static FluentResource> {
    let res = FluentResource::try_new(src.to_owned()).expect("valid FTL");
    let mut bundle = FluentBundle::new(vec![lang]);
    bundle.add_resource(&res).expect("add res");
    bundle
}

/// Translate a message `key` with optional arguments into the requested language.
/// Falls back to English if key missing.
pub fn tr(lang: Lang, key: &str, args: Option<&FluentArgs>) -> String {
    let bundle = match lang {
        Lang::En => &*EN_BUNDLE,
        Lang::Zh => &*ZH_BUNDLE,
        Lang::Ja => &*JA_BUNDLE,
    };
    if let Some(msg) = bundle.get_message(key) {
        if let Some(pattern) = msg.value() {
            let mut errors = vec![];
            let val = bundle.format_pattern(pattern, args.unwrap_or(&FluentArgs::new()), &mut errors);
            return val.into_owned();
        }
    }
    // fallback
    if let Some(msg) = EN_BUNDLE.get_message(key) {
        if let Some(pattern) = msg.value() {
            let mut errors = vec![];
            return EN_BUNDLE.format_pattern(pattern, args.unwrap_or(&FluentArgs::new()), &mut errors).into_owned();
        }
    }
    key.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn en_translation() {
        let mut args = FluentArgs::new();
        args.set("port", FluentValue::from(43300));
        let msg = tr(Lang::En, "init-success", Some(&args));
        assert!(msg.contains("43300"));
    }
} 