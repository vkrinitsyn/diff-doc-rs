//! The messages' language: its own test binary, as set_lang is the process's

use diff_doc::{set_lang, DocError};
use diff_doc::generic::DocIndex;

/// set_lang's language, a locale taken by its language, English for one without
/// messages; the code the same in each
#[cfg(feature = "i18n")]
#[test]
fn messages_follow_set_lang() {
    let e = DocError::PathNotFound { path: DocIndex::Name("k".into()) };
    set_lang("ru");
    assert_eq!(e.to_string(), "DD-E001 путь не найден: \"k\"");
    set_lang("de_DE.UTF-8");
    assert_eq!(e.to_string(), "DD-E001 Pfad nicht gefunden: \"k\"");
    set_lang("xx");
    assert_eq!(e.to_string(), "DD-E001 path not found: \"k\"");
    set_lang("en");
}

/// without the i18n feature: English, whatever set_lang
#[cfg(not(feature = "i18n"))]
#[test]
fn messages_are_english() {
    set_lang("ru");
    let e = DocError::PathNotFound { path: DocIndex::Name("k".into()) };
    assert_eq!(e.to_string(), "DD-E001 path not found: \"k\"");
}
