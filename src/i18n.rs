//! [i18n] The language of diff_doc's messages - its errors (error.rs).
//!
//! The first of: [set_lang]; the environment's LANG; English. A locale such as
//! `ru_RU.UTF-8` is taken by its language, and a language without its messages is
//! English.
//!
//! Only with the `i18n` feature, a default one; without it the messages are English,
//! and set_lang changes nothing.
//!
//! The messages are in locales/*.yml, each with every language: en, ru, and - not
//! reviewed - fr, de, zh (Simplified Chinese), ja. An error's starts with its code,
//! `DD-Ennn`, the same in every language.

#[cfg(feature = "i18n")]
use std::sync::RwLock;

/// set_lang's
#[cfg(feature = "i18n")]
static LANG: RwLock<Option<String>> = RwLock::new(None);

/// The language of the messages from here on: `en`, `ru`, `fr`, `de`, `zh`, `ja`, or a
/// locale of one (`ru_RU.UTF-8`). Wins over LANG. Without the i18n feature, nothing.
pub fn set_lang(lang: &str) {
    #[cfg(feature = "i18n")]
    if let Ok(mut l) = LANG.write() {
        *l = Some(language(lang));
    }
    #[cfg(not(feature = "i18n"))]
    let _ = lang;
}

/// The language the messages are in now
#[cfg(feature = "i18n")]
pub(crate) fn lang() -> String {
    choose(LANG.read().ok().and_then(|l| l.clone()), std::env::var("LANG").ok())
}

#[cfg(feature = "i18n")]
fn choose(set: Option<String>, env: Option<String>) -> String {
    set.or_else(|| env.map(|e| language(&e)))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "en".to_string())
}

/// a locale's language: its first two letters, lower case
#[cfg(feature = "i18n")]
fn language(locale: &str) -> String {
    locale.trim().chars().take(2).collect::<String>().to_lowercase()
}

/// [i18n] The message `key` in the language of [lang], each %{name} placeholder filled
#[cfg(feature = "i18n")]
macro_rules! tr {
    ($key:expr $(, $name:ident = $value:expr)* $(,)?) => {
        rust_i18n::t!($key, locale = &$crate::i18n::lang() $(, $name = $value)*).into_owned()
    };
}

/// The message `key` in English, each %{name} placeholder filled
#[cfg(not(feature = "i18n"))]
macro_rules! tr {
    ($key:expr $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::fill($key, &[$((stringify!($name), $value.to_string())),*])
    };
}

// the English of locales/*.yml: en(key), from build.rs
#[cfg(not(feature = "i18n"))]
include!(concat!(env!("OUT_DIR"), "/en.rs"));

/// the English message `key` - the key itself when there is none, as rust-i18n's -
/// with its placeholders filled
#[cfg(not(feature = "i18n"))]
pub(crate) fn fill(key: &str, values: &[(&str, String)]) -> String {
    let mut m = match en(key) {
        "" => key.to_string(),
        m => m.to_string(),
    };
    for (name, value) in values {
        m = m.replace(&format!("%{{{}}}", name), value);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// [i18n] set_lang over LANG, LANG over English
    #[cfg(feature = "i18n")]
    #[test]
    fn the_language_is_the_first_one_given() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(choose(s("de"), s("ru_RU.UTF-8")), "de");
        assert_eq!(choose(None, s("ru_RU.UTF-8")), "ru");
        assert_eq!(choose(None, None), "en");
        assert_eq!(choose(None, s("")), "en");
        assert_eq!(language("JA_jp"), "ja");
    }

    /// [i18n] a language without messages, or a key without one in it, is English
    #[cfg(feature = "i18n")]
    #[test]
    fn a_message_falls_back_to_english() {
        let en = rust_i18n::t!("txt.append_eol", locale = "en", line = 1).into_owned();
        assert!(en.starts_with("DD-E"), "{}", en);
        assert_eq!(rust_i18n::t!("txt.append_eol", locale = "xx", line = 1).into_owned(), en);
        assert_ne!(rust_i18n::t!("txt.append_eol", locale = "ru", line = 1).into_owned(), en);
    }

    /// without the i18n feature, the English of locales/ - each placeholder filled -
    /// whatever set_lang or LANG
    #[cfg(not(feature = "i18n"))]
    #[test]
    fn without_i18n_a_message_is_english() {
        set_lang("ru");
        let all = messages();
        for (key, langs) in &all {
            assert_eq!(en(key), langs["en"], "{}", key);
        }
        assert_eq!(tr!("txt.append_eol", line = 4), all["txt.append_eol"]["en"].replace("%{line}", "4"));
        assert_eq!(fill("no.such", &[]), "no.such");
    }

    /// locales/*.yml, as key => language => message
    fn messages() -> BTreeMap<String, BTreeMap<String, String>> {
        let dir = format!("{}/locales", env!("CARGO_MANIFEST_DIR"));
        let mut all = BTreeMap::new();
        for f in std::fs::read_dir(&dir).unwrap() {
            let f = f.unwrap().path();
            let y: BTreeMap<String, serde_json::Value> =
                serde_saphyr::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
            for (k, v) in y {
                if k == "_version" {
                    continue;
                }
                let langs = v.as_object().unwrap_or_else(|| panic!("{}: {}", f.display(), k)).iter()
                    .map(|(l, m)| (l.clone(), m.as_str().unwrap().to_string())).collect();
                assert!(all.insert(k.clone(), langs).is_none(), "{} twice", k);
            }
        }
        all
    }

    /// [i18n] every message in every language, with the same placeholders; a message's
    /// code - an error's DD-E, an intersection's DD-I - the same in every language, and
    /// no two messages with one code
    #[test]
    fn every_message_is_in_every_language() {
        let placeholders = |m: &str| {
            let mut p: Vec<String> = m.split("%{").skip(1).map(|r| r.split('}').next().unwrap().to_string()).collect();
            p.sort();
            p
        };
        let mut codes = BTreeMap::new();
        for (key, langs) in messages() {
            let en = &langs["en"];
            assert_eq!(langs.keys().map(String::as_str).collect::<Vec<_>>(), ["de", "en", "fr", "ja", "ru", "zh"], "{}", key);
            let code = en.split(' ').next().filter(|c| c.starts_with("DD-"));
            for (l, m) in &langs {
                assert_eq!(placeholders(m), placeholders(en), "{} {}", key, l);
                assert_eq!(m.split(' ').next().filter(|c| c.starts_with("DD-")), code, "{} {}: the code", key, l);
            }
            if let Some(c) = code {
                assert!(codes.insert(c.to_string(), key.clone()).is_none(), "{} for {} and {}", c, codes[c], key);
            }
        }
        assert!(codes.len() >= 15, "{}", codes.len());
    }

    /// [i18n] every key the code asks for, tr!("key"), is in locales/, and every
    /// message there asked for
    #[test]
    fn every_key_used_has_its_message_and_every_message_a_use() {
        let all = messages();
        let mut used = BTreeSet::new();
        let dir = format!("{}/src", env!("CARGO_MANIFEST_DIR"));
        for f in std::fs::read_dir(&dir).unwrap() {
            let f = f.unwrap().path();
            let src = std::fs::read_to_string(&f).unwrap();
            for call in src.split("tr!(\"").skip(1) {
                let key = call.split('"').next().unwrap();
                // the macro's own definition and docs
                if key.contains('.') && !key.contains(' ') {
                    assert!(all.contains_key(key), "{}: no message {}", f.display(), key);
                    used.insert(key.to_string());
                }
            }
        }
        let unused: Vec<&String> = all.keys().filter(|k| !used.contains(*k)).collect();
        assert!(unused.is_empty(), "messages nothing asks for: {:?}", unused);
    }
}
