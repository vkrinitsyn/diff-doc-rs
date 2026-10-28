//! BSON, as DocumentDB and MongoDB store a document, as a GenericValue, and back:
//! generic.rs's from_bson and to_bson.
//!
//! JSON's types are themselves; the mapping of the rest is
//! [extended JSON](https://www.mongodb.com/docs/manual/reference/mongodb-extended-json/),
//! relaxed, so each reads back as the BSON type it was:
//! - an int32: its number; a double: its number with a fraction or an exponent, `1.0`;
//!   an int64: its number out of int32's range, else `{"$numberLong": "7"}`
//! - a double not finite: `{"$numberDouble": "NaN"}`, `"Infinity"`, `"-Infinity"`
//! - an ObjectId `{"$oid": "..."}`, a date `{"$date": "2024-05-01T10:00:00Z"}`, a decimal
//!   `{"$numberDecimal": "1.10"}`, binary `{"$binary": {"base64": ..., "subType": "00"}}`,
//!   a timestamp, a regex, code, min and max keys: each as extended JSON has it
//!
//! So the diff of `{"_id": {"$oid": ...}, "at": {"$date": ...}}` is a change of the
//! `$oid` or the `$date`, and a document exported as extended JSON (mongoexport) is,
//! by from_json, the same GenericValue as its BSON.
//! Not kept: the order of the keys - a GenericValue's map has none - to_bson writes
//! them sorted, the document's `_id` first, so each node writes the same bytes.
//! An integer past int64 has no BSON type: to_bson fails on it.
//! A map of another `$` key, `{"$set": ...}`, is a document as any other.
//!
//! c/bson_doc.c is the same mapping in C: what one reads, the other writes back.

use bson::{Bson, Document};

use crate::generic::{GenericValue, NumericString};
use crate::DocError;

/// the last millisecond of 9999-12-31: a later date has no RFC 3339 of 4 digits
const MAX_RFC3339: i64 = 253_402_300_799_999;

/// read as from_json reads the same text: a number, or a string of one, as its Numeric
fn json(v: serde_json::Value) -> Result<GenericValue, bson::error::Error> {
    Ok(serde_json::from_value(v)?)
}

fn value(b: Bson) -> Result<GenericValue, bson::error::Error> {
    Ok(match b {
        // `{:?}`: with a fraction or an exponent, so a double reads back as one
        Bson::Double(f) if f.is_finite() => serde_json::from_str(&format!("{:?}", f))?,
        Bson::Int32(i) => GenericValue::Numeric(NumericString(i.to_string())),
        Bson::Int64(i) if i32::try_from(i).is_err() => GenericValue::Numeric(NumericString(i.to_string())),
        Bson::Int64(i) => json(serde_json::json!({"$numberLong": i.to_string()}))?,
        Bson::String(s) => GenericValue::StringValue(s),
        Bson::Boolean(b) => GenericValue::Boolean(b),
        Bson::Null => GenericValue::Null,
        Bson::Array(a) => GenericValue::Array(a.into_iter().map(value).collect::<Result<_, _>>()?),
        Bson::Document(d) => document(d)?,
        // RFC 3339 of years 1970 to 9999, as relaxed extended JSON has it - the bson crate
        // would write a later one as 9999-12-31T23:59:59.999999999Z
        Bson::DateTime(d) => match d.timestamp_millis() {
            0..=MAX_RFC3339 => json(serde_json::json!({"$date": d.try_to_rfc3339_string()?}))?,
            ms => json(serde_json::json!({"$date": {"$numberLong": ms.to_string()}}))?,
        },
        // the rest as relaxed extended JSON has it
        b => json(b.into_relaxed_extjson())?,
    })
}

fn document(d: Document) -> Result<GenericValue, bson::error::Error> {
    Ok(GenericValue::Map(d.into_iter().map(|(k, v)| Ok((k, value(v)?))).collect::<Result<_, bson::error::Error>>()?))
}

/// `at`: the path to the value, for the error of a number BSON has no type for
fn bson(v: &GenericValue, at: &str) -> Result<Bson, DocError> {
    Ok(match v {
        GenericValue::Numeric(NumericString(n)) => {
            if let Ok(i) = n.parse::<i32>() {
                Bson::Int32(i)
            } else if let Ok(i) = n.parse::<i64>() {
                Bson::Int64(i)
            } else if n.strip_prefix(['+', '-']).unwrap_or(n).bytes().all(|b| b.is_ascii_digit()) {
                // an integer past int64: as a double it would lose its digits
                return Err(DocError::BsonNumber { value: n.clone(), at: at.to_string() });
            } else {
                Bson::Double(n.parse().map_err(|_| DocError::BsonNumber { value: n.clone(), at: at.to_string() })?)
            }
        }
        GenericValue::StringValue(s) => Bson::String(s.clone()),
        GenericValue::Boolean(b) => Bson::Boolean(*b),
        GenericValue::Null => Bson::Null,
        GenericValue::Array(a) => Bson::Array(a.iter().enumerate()
            .map(|(i, v)| bson(v, &format!("{}/{}", at, i))).collect::<Result<_, _>>()?),
        // a type of extended JSON: the bson crate reads it
        GenericValue::Map(m) if m.keys().any(|k| TYPES.contains(&k.as_str())) => Bson::try_from(extjson(v, false, at)?)?,
        GenericValue::Map(m) => Bson::Document(map(m, at)?),
    })
}

/// the keys of extended JSON's types, as the bson crate reads them
const TYPES: [&str; 16] = ["$oid", "$symbol", "$regularExpression", "$numberInt", "$numberLong", "$numberDouble",
    "$numberDecimal", "$binary", "$uuid", "$code", "$timestamp", "$date", "$minKey", "$maxKey", "$dbPointer", "$undefined"];

/// a value of extended JSON's as the bson crate reads it: its numbers are strings,
/// `{"$numberLong": "7"}`, that from_json has read as numbers - all but a timestamp's
/// and the keys' `1`; `$scope` is a document, of BSON's own values
fn extjson(v: &GenericValue, numbers: bool, at: &str) -> Result<serde_json::Value, DocError> {
    use serde_json::Value;
    Ok(match v {
        GenericValue::Numeric(NumericString(n)) if !numbers => Value::String(n.clone()),
        GenericValue::Array(a) => Value::Array(a.iter().map(|v| extjson(v, numbers, at)).collect::<Result<_, _>>()?),
        GenericValue::Map(m) => Value::Object(m.iter().map(|(k, v)| Ok((k.clone(), match k.as_str() {
            "$scope" => Bson::Document(map_of(v, at)?).into_canonical_extjson(),
            "$timestamp" | "$minKey" | "$maxKey" => extjson(v, true, at)?,
            _ => extjson(v, numbers, at)?,
        }))).collect::<Result<_, DocError>>()?),
        v => serde_json::to_value(v)?,
    })
}

/// a map's document; any other value, none
fn map_of(v: &GenericValue, at: &str) -> Result<Document, DocError> {
    match bson(v, at)? {
        Bson::Document(d) => Ok(d),
        // not a map, or one of extended JSON's types
        _ => Err(DocError::BsonRoot),
    }
}

fn map(m: &std::collections::HashMap<String, GenericValue>, at: &str) -> Result<Document, DocError> {
    let mut keys: Vec<&String> = m.keys().collect();
    // the document's _id first, as DocumentDB writes it
    keys.sort_by_key(|k| (!(at.is_empty() && k.as_str() == "_id"), *k));
    keys.into_iter().map(|k| Ok((k.clone(), bson(&m[k], &format!("{}/{}", at, k))?))).collect()
}

pub(crate) fn from_bson_document(d: Document) -> Result<GenericValue, bson::error::Error> {
    document(d)
}

pub(crate) fn from_bson(bytes: &[u8]) -> Result<GenericValue, bson::error::Error> {
    document(Document::from_reader(bytes)?)
}

pub(crate) fn to_bson_document(value: &GenericValue) -> Result<Document, DocError> {
    map_of(value, "")
}

pub(crate) fn to_bson(value: &GenericValue) -> Result<Vec<u8>, DocError> {
    Ok(to_bson_document(value)?.to_vec()?)
}

#[cfg(test)]
mod tests {
    use bson::{doc, oid::ObjectId, Binary, DateTime, Decimal128, Timestamp};

    use super::*;
    use crate::generic::from_json;

    fn json(s: &str) -> GenericValue {
        from_json(s).unwrap()
    }

    /// the mapping: JSON's types as they are, the rest as extended JSON
    #[test]
    fn bson_as_a_generic_value() {
        let id = ObjectId::parse_str("65f1a2b3c4d5e6f708192a3b").unwrap();
        let d = doc! {
            "_id": id, "name": "api", "on": true, "none": null,
            "i32": 7, "i64": 7_i64, "big": 5_000_000_000_i64, "f": 1.0, "e": 1e300, "nan": f64::NAN,
            "at": DateTime::from_millis(1_714_557_600_000), "dec": "1.10".parse::<Decimal128>().unwrap(),
            "bin": Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: vec![1, 2] },
            "ts": Timestamp { time: 1, increment: 2 }, "tags": ["a", {"k": 1}],
        };
        let v = from_bson(&d.to_vec().unwrap()).unwrap();
        assert_eq!(v, json(r#"{"_id": {"$oid": "65f1a2b3c4d5e6f708192a3b"}, "name": "api", "on": true, "none": null,
            "i32": 7, "i64": {"$numberLong": "7"}, "big": 5000000000, "f": 1.0, "e": 1e300, "nan": {"$numberDouble": "NaN"},
            "at": {"$date": "2024-05-01T10:00:00Z"}, "dec": {"$numberDecimal": "1.10"},
            "bin": {"$binary": {"base64": "AQI=", "subType": "00"}},
            "ts": {"$timestamp": {"t": 1, "i": 2}}, "tags": ["a", {"k": 1}]}"#));
        // each reads back as the type it was, NaN aside: it equals nothing
        let back = to_bson_document(&v).unwrap();
        let mut d = d;
        assert!(back.get_f64("nan").unwrap().is_nan());
        d.remove("nan");
        let mut back = back;
        back.remove("nan");
        for (k, b) in d.iter() {
            assert_eq!(back.get(k), Some(b), "{}", k);
        }
        assert_eq!(from_bson_document(d.clone()).unwrap(), from_bson(&to_bson(&from_bson_document(d).unwrap()).unwrap()).unwrap());
    }

    /// regex, code with its scope, the keys and an old date: read back as they were
    #[test]
    fn the_other_types_read_back() {
        let d = doc! {
            "re": bson::Regex { pattern: "^a".try_into().unwrap(), options: "i".try_into().unwrap() },
            "code": bson::JavaScriptCodeWithScope { code: "x".into(), scope: doc! {"n": 1, "f": 2.5} },
            "min": Bson::MinKey, "max": Bson::MaxKey, "old": DateTime::from_millis(-1),
            "inf": f64::NEG_INFINITY, "sym": Bson::Symbol("s".into()),
        };
        let v = from_bson_document(d.clone()).unwrap();
        assert_eq!(to_bson_document(&v).unwrap(), d, "{:?}", v);
        assert_eq!(v, from_json(&crate::generic::to_json(&v).unwrap()).unwrap());
    }

    /// _id first in the document, the rest sorted; a map of another `$` key is a document
    #[test]
    fn keys_in_order() {
        let d = to_bson_document(&json(r#"{"b": 1, "_id": 2, "a": {"_id": 3, "Z": 4}, "$set": {"x": 5}}"#)).unwrap();
        let keys: Vec<&String> = d.keys().collect();
        assert_eq!(keys, ["_id", "$set", "a", "b"]);
        assert_eq!(d.get_document("a").unwrap().keys().collect::<Vec<_>>(), ["Z", "_id"]);
        assert_eq!(d.get_document("$set").unwrap(), &doc! {"x": 5});
    }

/// the GenericValue as c/test_bson.c dumps it: kind, value, map keys sorted
    pub(crate) fn dump(v: &GenericValue) -> String {
        match v {
            GenericValue::Null => "z".into(),
            GenericValue::Boolean(b) => format!("b{}", b),
            GenericValue::Numeric(n) => format!("n{:?}", n.0),
            GenericValue::StringValue(s) => format!("s{:?}", s),
            GenericValue::Array(a) => format!("[{}]", a.iter().map(dump).collect::<Vec<_>>().join(",")),
            GenericValue::Map(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                format!("{{{}}}", keys.iter().map(|k| format!("{:?}:{}", k, dump(&m[*k]))).collect::<Vec<_>>().join(","))
            }
        }
    }

    /// every BSON type, for c/test_bson.c: c/testdata/types.bson as to_bson writes it,
    /// types.txt the GenericValue it reads as. BSON_FIXTURE=write writes them anew
    #[test]
    fn the_c_fixture() {
        let d = doc! {
            "_id": ObjectId::parse_str("65f1a2b3c4d5e6f708192a3b").unwrap(), "s": "text", "s7": "7", "t": true, "f": false, "z": null,
            "i32": -7, "i64": 7_i64, "i64big": -5_000_000_000_i64,
            "d": [1.0, 0.1, -0.0, 1e300, 1e16, 1e15, 1.5e-7, 123.456, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MIN_POSITIVE],
            "at": [DateTime::from_millis(1_714_557_600_000), DateTime::from_millis(253_402_300_799_999), DateTime::from_millis(1_714_557_600_120), DateTime::from_millis(1),
                DateTime::from_millis(-1), DateTime::from_millis(253_402_300_800_000), DateTime::from_millis(0)],
            "dec": (["1.10", "-0", "0", "1E+10", "NaN", "-Infinity", "0.000001", "1E-7", "-12345678901234567890123456789012.34",
                "9.999999999999999999999999999999999E+6144", "1E-6176", "100"].map(|s| s.parse::<Decimal128>().unwrap()).to_vec()),
            "bin": Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: vec![0, 1, 2, 250, 251, 255] },
            "uuid": Binary { subtype: bson::spec::BinarySubtype::Uuid, bytes: (0..16).collect() },
            "bin0": Binary { subtype: bson::spec::BinarySubtype::UserDefined(0x80), bytes: vec![] },
            "ts": Timestamp { time: 4_000_000_000, increment: 2 },
            "re": bson::Regex { pattern: "^a\\d".try_into().unwrap(), options: "mi".try_into().unwrap() },
            "code": Bson::JavaScriptCode("f()".into()),
            "scope": bson::JavaScriptCodeWithScope { code: "x".into(), scope: doc! {"n": 1, "l": 2_i64, "f": 2.0, "s": "3"} },
            "min": Bson::MinKey, "max": Bson::MaxKey, "sym": Bson::Symbol("s".into()), "undef": Bson::Undefined,
            "ptr": Bson::try_from(serde_json::json!({"$dbPointer": {"$ref": "db.c", "$id": {"$oid": "65f1a2b3c4d5e6f708192a3b"}}})).unwrap(),
            "nested": {"a": [1, {"b": [[], {}]}], "_id": 2},
        };
        let v = from_bson_document(d).unwrap();
        let bytes = to_bson(&v).unwrap();
        let text = dump(&v) + "\n";
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/c/testdata");
        if std::env::var("BSON_FIXTURE").is_ok_and(|w| w == "write") {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(format!("{}/types.bson", dir), &bytes).unwrap();
            std::fs::write(format!("{}/types.txt", dir), &text).unwrap();
        }
        assert_eq!(std::fs::read(format!("{}/types.bson", dir)).unwrap(), bytes, "BSON_FIXTURE=write to write it anew");
        assert_eq!(std::fs::read_to_string(format!("{}/types.txt", dir)).unwrap(), text);
        assert_eq!(from_bson(&bytes).unwrap(), v);
    }

    /// the decimals of c/test_bson.c's test_decimals: the same text of each, or an error
    #[test]
    fn decimals_as_c_reads_them() {
        for (s, want) in [("1.0E+3", "1.0E+3"), ("12e-2", "0.12"), ("-0.0", "-0.0"), ("1E+6112", "1.0E+6112"),
            ("0E-7000", "0E-6176"), ("+inf", "Infinity"), ("-nan", "NaN"), ("1.10", "1.10"), ("10E-6177", "1E-6176"),
            ("000123", "123"), (".5", "0.5")] {
            assert_eq!(s.parse::<Decimal128>().unwrap().to_string(), want, "{}", s);
        }
        for s in ["1E-6177", "12345678901234567890123456789012345", "1x", "1e", "1E+6145"] {
            assert!(s.parse::<Decimal128>().is_err(), "{}", s);
        }
        // an integer past int64, of any length, as c/bson_doc.c has it
        assert!(matches!(to_bson(&json(&format!(r#"{{"a": {}}}"#, "9".repeat(50)))), Err(DocError::BsonNumber { .. })));
    }

    /// a number read from JSON: int32, int64 or double by its value and form
    #[test]
    fn json_numbers_as_bson() {
        let d = to_bson_document(&json(r#"{"a": 1, "b": 5000000000, "c": 1.5, "d": -0.0}"#)).unwrap();
        assert_eq!(d, doc! {"a": 1, "b": 5_000_000_000_i64, "c": 1.5, "d": -0.0});
    }

    /// values BSON cannot hold
    #[test]
    fn not_bson() {
        let x = |s: &str| to_bson(&json(s)).unwrap_err();
        assert!(matches!(x("[1]"), DocError::BsonRoot));
        assert!(matches!(x("1"), DocError::BsonRoot));
        assert!(matches!(x(r#"{"$oid": "65f1a2b3c4d5e6f708192a3b"}"#), DocError::BsonRoot));
        assert!(matches!(x(r#"{"a": [1, 18446744073709551616]}"#),
            DocError::BsonNumber { value, at } if value == "18446744073709551616" && at == "/a/1"));
        assert!(matches!(x(r#"{"a": {"$oid": "nope"}}"#), DocError::Format { format: crate::Format::Bson, .. }));
        for e in [x("[1]"), x(r#"{"a": 18446744073709551616}"#)] {
            assert!(e.to_string().starts_with("DD-E50"), "{}", e);
        }
        let e: DocError = from_bson(&[1, 2]).unwrap_err().into();
        assert!(e.to_string().starts_with("BSON: "), "{}", e);
    }
}
