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
    #[allow(unused_mut)]
    let mut errors: Vec<(DocError, Format, &str)> = vec![
        (from_json("{").unwrap_err().into(), Format::Json, "JSON: "),
        (from_yaml("a: [").unwrap_err().into(), Format::Yaml, "YAML: "),
        (from_toml("a = ").unwrap_err().into(), Format::Toml, "TOML: "),
        (from_xml("<a>").unwrap_err().into(), Format::Xml, "XML: "),
    ];
    #[cfg(feature = "bson")]
    errors.push((from_bson(&[5, 0, 0, 0]).unwrap_err().into(), Format::Bson, "BSON: "));
    for (e, format, code) in errors {
        assert!(matches!(&e, DocError::Format { format: f, .. } if *f == format), "{:?}", e);
        assert!(e.to_string().starts_with(code), "{}", e);
        assert!(std::error::Error::source(&e).is_some());
    }
}

/// DocumentDB's documents: A changes a date and an int64, B adds a tag and removes a
/// field; the ObjectId, the decimal and the int64 stay the types they were
#[cfg(feature = "bson")]
#[test]
fn bson() {
    use diff_doc::bson::{doc, oid::ObjectId, DateTime, Decimal128};
    let id = ObjectId::parse_str("65f1a2b3c4d5e6f708192a3b").unwrap();
    let price: Decimal128 = "9.90".parse().unwrap();
    let at = |ms| DateTime::from_millis(ms);
    let read = |d: diff_doc::bson::Document| from_bson(&d.to_vec().unwrap()).unwrap();
    let base = read(doc! {"_id": id, "price": price, "seen": 1_i64, "at": at(1_000), "tags": ["a"], "old": true});
    let a = read(doc! {"_id": id, "price": price, "seen": 2_i64, "at": at(2_000), "tags": ["a"], "old": true});
    let b = read(doc! {"_id": id, "price": price, "seen": 1_i64, "at": at(1_000), "tags": ["a", "b"]});
    let pa = Mismatch::new(&base, &a).unwrap();
    let pb = Mismatch::new(&base, &b).unwrap();
    assert!(!pa.is_intersect(&pb).unwrap());
    let result = doc! {"_id": id, "at": at(2_000), "price": price, "seen": 2_i64, "tags": ["a", "b"]};
    for (first, second) in [(&pa, &pb), (&pb, &pa)] {
        let mut doc = base.clone();
        assert!(first.apply_mut(&mut doc, true).unwrap().is_empty());
        assert!(second.apply_mut(&mut doc, true).unwrap().is_empty());
        assert_eq!(to_bson_document(&doc).unwrap(), result);
        assert_eq!(to_bson(&doc).unwrap(), result.to_vec().unwrap());
    }
}
