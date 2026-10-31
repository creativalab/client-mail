//! UI translations. Each language is a `locales/<code>.toml` file
//! embedded in the binary; missing keys fall back to English.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lang {
    pub code: &'static str,
    /// Language name in the language itself.
    pub native: &'static str,
    pub rtl: bool,
    source: &'static str,
}

pub const LANGS: [Lang; 10] = [
    Lang { code: "en", native: "English", rtl: false, source: include_str!("../locales/en.toml") },
    Lang { code: "it", native: "Italiano", rtl: false, source: include_str!("../locales/it.toml") },
    Lang { code: "fr", native: "Français", rtl: false, source: include_str!("../locales/fr.toml") },
    Lang { code: "de", native: "Deutsch", rtl: false, source: include_str!("../locales/de.toml") },
    Lang { code: "es", native: "Español", rtl: false, source: include_str!("../locales/es.toml") },
    Lang { code: "pt", native: "Português", rtl: false, source: include_str!("../locales/pt.toml") },
    Lang { code: "zh", native: "中文（简体）", rtl: false, source: include_str!("../locales/zh.toml") },
    Lang { code: "ja", native: "日本語", rtl: false, source: include_str!("../locales/ja.toml") },
    Lang { code: "ko", native: "한국어", rtl: false, source: include_str!("../locales/ko.toml") },
    Lang { code: "ar", native: "العربية", rtl: true, source: include_str!("../locales/ar.toml") },
];

/// Support address shown on unexpected errors.
pub const SUPPORT_EMAIL: &str = "support@ferrowl.com";

/// Generic message for internal errors.
pub fn generic_error() -> String {
    tf("err_generic", &[("email", SUPPORT_EMAIL)])
}

static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn tables() -> &'static Vec<HashMap<String, &'static str>> {
    static TABLES: OnceLock<Vec<HashMap<String, &'static str>>> = OnceLock::new();
    TABLES.get_or_init(|| {
        LANGS
            .iter()
            .map(|l| {
                let parsed: toml::Table = toml::from_str(l.source)
                    .unwrap_or_else(|e| panic!("locales/{}.toml invalid: {e}", l.code));
                parsed
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let s = v.as_str()?.to_string();
                        Some((k, &*Box::leak(s.into_boxed_str())))
                    })
                    .collect()
            })
            .collect()
    })
}

/// System language, if supported (English otherwise).
pub fn system_lang() -> &'static Lang {
    let loc = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    let code = loc.split(['-', '_', '.']).next().unwrap_or("en");
    LANGS.iter().find(|l| l.code == code).unwrap_or(&LANGS[0])
}

/// Sets the language: `None` or "auto" = system language.
pub fn set(code: Option<&str>) {
    let lang = match code {
        Some(c) if c != "auto" => LANGS.iter().find(|l| l.code == c).unwrap_or(system_lang()),
        _ => system_lang(),
    };
    let idx = LANGS.iter().position(|l| l.code == lang.code).unwrap_or(0);
    CURRENT.store(idx, Ordering::Relaxed);
}

pub fn current() -> &'static Lang {
    &LANGS[CURRENT.load(Ordering::Relaxed)]
}

pub fn is_rtl() -> bool {
    current().rtl
}

/// Translated text for `key`.
pub fn t(key: &str) -> &'static str {
    let tables = tables();
    let idx = CURRENT.load(Ordering::Relaxed);
    tables[idx]
        .get(key)
        .or_else(|| tables[0].get(key))
        .copied()
        .unwrap_or_else(|| Box::leak(key.to_string().into_boxed_str()))
}

/// Translated text with `{name}` placeholders substituted.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(key).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every language must have all English keys, with the same placeholders.
    #[test]
    fn all_locales_complete() {
        let tables = tables();
        let en = &tables[0];
        let placeholders = |s: &str| {
            let mut v: Vec<String> = s
                .split('{')
                .skip(1)
                .filter_map(|p| p.split_once('}').map(|(n, _)| n.to_string()))
                .collect();
            v.sort();
            v
        };
        let mut problems = Vec::new();
        for (lang, table) in LANGS.iter().zip(tables.iter()) {
            for (k, v) in en {
                match table.get(k) {
                    None => problems.push(format!("{}: missing \"{k}\"", lang.code)),
                    Some(tr) if k.ends_with("_fmt") => {
                        let _ = tr;
                    }
                    Some(tr) if placeholders(tr) != placeholders(v) => {
                        problems.push(format!("{}: placeholders differ in \"{k}\"", lang.code))
                    }
                    _ => {}
                }
            }
            for k in table.keys() {
                if !en.contains_key(k) {
                    problems.push(format!("{}: extra key \"{k}\"", lang.code));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn switch_language() {
        set(Some("it"));
        assert_eq!(t("compose"), "Scrivi");
        set(Some("en"));
        assert_eq!(t("compose"), "Compose");
        assert_eq!(tf("status_synced", &[("time", "09:42")]), "Synced · 09:42");
    }
}
