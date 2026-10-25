//! The same 3-way merge in each format: a change of A and one of B to one base, in
//! either order, give the result - and an error of a format's parser is a DocError

use std::fs::read_to_string;
use diff_doc::{DocError, Format, MismatchDoc, MismatchDocMut};
use diff_doc::diff::Mismatch;
use diff_doc::generic::*;

/// tests/formats/`ext`: base, a, b and result, parsed by `parse`
fn merge<E: std::fmt::Debug>(ext: &str, parse: fn(&str) -> Result<GenericValue, E>) -> GenericValue {
    let read = |name: &str| parse(&read_to_string(format!("tests/formats/{}/{}.{}", ext, name, ext)).unwrap()).unwrap();
    let base = read("base");
    let pa = Mismatch::new(&base, &read("a")).unwrap();
    let pb = Mismatch::new(&base, &read("b")).unwrap();
    println!("{} A: {:?}\n{} B: {:?}", ext, pa, ext, pb);
    assert!(!pa.is_intersect(&pb).unwrap());
    let result = read("result");
    for (first, second) in [(&pa, &pb), (&pb, &pa)] {
        let mut doc = base.clone();
        assert!(first.apply_mut(&mut doc, true).unwrap().is_empty());
        assert!(second.apply_mut(&mut doc, true).unwrap().is_empty());
        assert_eq!(doc, result, "{}", ext);
    }
    result
}

#[test]
fn yaml() {
    let result = merge("yaml", from_yaml);
    assert_eq!(from_yaml(&to_yaml(&result).unwrap()).unwrap(), result);
}

#[test]
fn toml() {
    let result = merge("toml", from_toml);
    assert_eq!(from_toml(&to_toml(&result).unwrap()).unwrap(), result);
}

/// an attribute, a value changed by A, an element repeated once more by B
#[test]
fn xml() {
    let result = merge("xml", from_xml);
    assert_eq!(from_xml(&to_xml(&result).unwrap()).unwrap(), result);
}

/// each format's parse error converts to DocError::Format: the format, and the parser's
/// message as it is
#[test]
fn a_parse_error_is_a_doc_error() {
    let errors: Vec<(DocError, Format, &str)> = vec![
        (from_json("{").unwrap_err().into(), Format::Json, "JSON: "),
        (from_yaml("a: [").unwrap_err().into(), Format::Yaml, "YAML: "),
        (from_toml("a = ").unwrap_err().into(), Format::Toml, "TOML: "),
        (from_xml("<a>").unwrap_err().into(), Format::Xml, "XML: "),
    ];
    for (e, format, code) in errors {
        assert!(matches!(&e, DocError::Format { format: f, .. } if *f == format), "{:?}", e);
        assert!(e.to_string().starts_with(code), "{}", e);
        assert!(std::error::Error::source(&e).is_some());
    }
}
