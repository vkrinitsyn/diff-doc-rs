//! Patches written by hand, as they travel: a list of hunks, each a path - `{"n": name}`
//! in a map, `{"i": index}` in an array - and an action. Each action on maps and arrays,
//! and each error of a patch that does not fit its document.

use diff_doc::{DocError, LineOp, Mismatches, MismatchDoc, MismatchDocCow, MismatchDocMut};
use diff_doc::diff::Mismatch;
use diff_doc::generic::{from_json, DocIndex, GenericValue};
use diff_doc::txt::{self, DiffOp};

fn doc(json: &str) -> GenericValue {
    from_json(json).unwrap()
}

/// `hunks` applied to `json`, stopping at the first error
fn apply(json: &str, hunks: &str) -> Result<GenericValue, DocError> {
    let patch: Mismatch = serde_json::from_str(hunks).unwrap();
    let mut d = doc(json);
    patch.apply_mut(&mut d, true).map(|_| d)
}

#[test]
fn map_actions() {
    let base = r#"{"a": 1, "b": 2, "c": "x"}"#;
    assert_eq!(apply(base, r#"[{"p": [{"n": "d"}], "v": {"Insert": 4}}]"#).unwrap(), doc(r#"{"a": 1, "b": 2, "c": "x", "d": 4}"#));
    assert_eq!(apply(base, r#"[{"p": [{"n": "c"}], "v": "Remove"}]"#).unwrap(), doc(r#"{"a": 1, "b": 2}"#));
    // the value of a copied to e
    assert_eq!(apply(base, r#"[{"p": [{"n": "e"}], "v": {"Clone": {"n": "a"}}}]"#).unwrap(), doc(r#"{"a": 1, "b": 2, "c": "x", "e": 1}"#));
    assert_eq!(apply(base, r#"[{"p": [{"n": "a"}], "v": {"Swap": {"n": "b"}}}]"#).unwrap(), doc(r#"{"a": 2, "b": 1, "c": "x"}"#));
    // a swap with a name not there yet moves the value
    assert_eq!(apply(base, r#"[{"p": [{"n": "z"}], "v": {"Swap": {"n": "c"}}}]"#).unwrap(), doc(r#"{"a": 1, "b": 2, "z": "x"}"#));
}

#[test]
fn array_actions() {
    let base = "[10, 20, 30, 40]";
    let one = |hunk: &str| apply(base, &format!("[{}]", hunk)).unwrap();
    assert_eq!(one(r#"{"p": [{"i": 1}], "v": "Remove"}"#), doc("[10, 30, 40]"));
    assert_eq!(one(r#"{"p": [{"i": 0}], "v": {"Update": 11}}"#), doc("[11, 20, 30, 40]"));
    assert_eq!(one(r#"{"p": [{"i": 2}], "v": {"Insert": 25}}"#), doc("[10, 20, 25, 30, 40]"));
    assert_eq!(one(r#"{"p": [{"i": 0}], "v": {"Swap": {"i": 3}}}"#), doc("[40, 20, 30, 10]"));
    // the element at 3 copied in at 1
    assert_eq!(one(r#"{"p": [{"i": 1}], "v": {"Clone": {"i": 3}}}"#), doc("[10, 40, 20, 30, 40]"));
    // out of range, or onto itself: nothing
    for hunk in [r#"{"p": [{"i": 0}], "v": {"Swap": {"i": 9}}}"#, r#"{"p": [{"i": 2}], "v": {"Swap": {"i": 2}}}"#,
                 r#"{"p": [{"i": 9}], "v": {"Clone": {"i": 0}}}"#, r#"{"p": [{"i": 1}], "v": {"Clone": {"i": 1}}}"#] {
        assert_eq!(one(hunk), doc(base), "{}", hunk);
    }
}

/// a line diff of a string, in a map and in an array
#[test]
fn text_actions() {
    let base = r#"{"s": "one\ntwo", "l": ["a\nb"]}"#;
    let set = |path: &str| format!(r#"[{{"p": {}, "v": {{"UpdateTxt": [{{"Update": {{"index": 1, "value": "B"}}}}]}}}}]"#, path);
    assert_eq!(apply(base, &set(r#"[{"n": "s"}]"#)).unwrap(), doc(r#"{"s": "one\nB", "l": ["a\nb"]}"#));
    assert_eq!(apply(base, &set(r#"[{"n": "l"}, {"i": 0}]"#)).unwrap(), doc(r#"{"s": "one\ntwo", "l": ["a\nB"]}"#));
}

/// a nested path: the nodes on the way are followed
#[test]
fn nested_paths() {
    let base = r#"{"m": {"l": [{"k": 1}]}}"#;
    assert_eq!(apply(base, r#"[{"p": [{"n": "m"}, {"n": "l"}, {"i": 0}, {"n": "k"}], "v": {"Update": 2}}]"#).unwrap(),
               doc(r#"{"m": {"l": [{"k": 2}]}}"#));
}

/// a document, the hunks applied to it, and the error expected
type ErrorCase = (&'static str, &'static str, fn(&DocError) -> bool);

#[test]
fn errors() {
    let cases: Vec<ErrorCase> = vec![
        // a name or an index not there, on the way or for a text change or a source
        ("{}", r#"[{"p": [{"n": "x"}, {"n": "y"}], "v": "Remove"}]"#, |e| matches!(e, DocError::PathNotFound { path: DocIndex::Name(n) } if n == "x")),
        ("[1]", r#"[{"p": [{"i": 5}, {"n": "y"}], "v": "Remove"}]"#, |e| matches!(e, DocError::PathNotFound { path: DocIndex::Idx(5) })),
        ("{}", r#"[{"p": [{"n": "s"}], "v": {"UpdateTxt": []}}]"#, |e| matches!(e, DocError::PathNotFound { .. })),
        ("[]", r#"[{"p": [{"i": 0}], "v": {"UpdateTxt": []}}]"#, |e| matches!(e, DocError::PathNotFound { path: DocIndex::Idx(0) })),
        (r#"{"a": 1}"#, r#"[{"p": [{"n": "b"}], "v": {"Clone": {"n": "nope"}}}]"#, |e| matches!(e, DocError::PathNotFound { path: DocIndex::Name(n) } if n == "nope")),
        // a text change of a number
        (r#"{"s": 1}"#, r#"[{"p": [{"n": "s"}], "v": {"UpdateTxt": []}}]"#, |e| matches!(e, DocError::NotAString { .. })),
        ("[1]", r#"[{"p": [{"i": 0}], "v": {"UpdateTxt": []}}]"#, |e| matches!(e, DocError::NotAString { path: DocIndex::Idx(0) })),
        // a map's swap or clone by index, an array's by name
        (r#"{"a": 1}"#, r#"[{"p": [{"n": "a"}], "v": {"Swap": {"i": 0}}}]"#, |e| matches!(e, DocError::IndexMismatch { index: DocIndex::Idx(0) })),
        ("[1]", r#"[{"p": [{"i": 0}], "v": {"Swap": {"n": "a"}}}]"#, |e| matches!(e, DocError::IndexMismatch { .. })),
        ("[1]", r#"[{"p": [{"i": 0}], "v": {"Clone": {"n": "a"}}}]"#, |e| matches!(e, DocError::IndexMismatch { .. })),
        // a name in an array, an index in a map
        ("[1]", r#"[{"p": [{"n": "a"}], "v": "Remove"}]"#, |e| matches!(e, DocError::NotAContainer { path: DocIndex::Name(_) })),
        (r#"{"a": 1}"#, r#"[{"p": [{"i": 0}], "v": "Remove"}]"#, |e| matches!(e, DocError::NotAContainer { path: DocIndex::Idx(0) })),
        // the text change's own error
        (r#"{"s": "x"}"#, r#"[{"p": [{"n": "s"}], "v": {"UpdateTxt": [{"Remove": {"index": 9}}]}}]"#,
            |e| matches!(e, DocError::LineOutOfBounds { op: LineOp::Remove, index: 9, len: 1 })),
    ];
    for (json, hunks, expected) in cases {
        let e = apply(json, hunks).unwrap_err();
        assert!(expected(&e), "{} {}: {:?}", json, hunks, e);
        assert!(e.to_string().starts_with("DD-E"), "{}", e);
    }
}

/// fail_fast stops at the first error; otherwise all apply that can, and the errors
/// come back
#[test]
fn fail_fast_or_collect() {
    let hunks = r#"[{"p": [{"n": "x"}, {"n": "y"}], "v": "Remove"}, {"p": [{"n": "a"}], "v": {"Update": 2}},
                    {"p": [{"i": 0}], "v": "Remove"}]"#;
    let patch: Mismatch = serde_json::from_str(hunks).unwrap();
    let mut d = doc(r#"{"a": 1}"#);
    assert!(matches!(patch.apply_mut(&mut d, true), Err(DocError::PathNotFound { .. })));
    assert_eq!(d, doc(r#"{"a": 1}"#));
    let errors = patch.apply_mut(&mut d, false).unwrap();
    assert_eq!(errors.len(), 2, "{:?}", errors);
    assert_eq!(d, doc(r#"{"a": 2}"#));
}

/// the simplified text patch's ops, by hand, beyond the text
#[test]
fn txt_errors() {
    let text = "one\ntwo".to_string();
    let x = |op: DiffOp| txt::Mismatch(vec![op]).apply(&text).unwrap_err();
    assert!(matches!(x(DiffOp::Insert { index: 5, value: "x".into() }), DocError::LineOutOfBounds { op: LineOp::Insert, index: 5, len: 2 }));
    assert!(matches!(x(DiffOp::Update { index: 2, value: "x".into() }), DocError::LineOutOfBounds { op: LineOp::Update, .. }));
    assert!(matches!(x(DiffOp::Append { index: 2, pos: 0, value: "x".into() }), DocError::LineOutOfBounds { op: LineOp::Append, .. }));
    assert!(matches!(x(DiffOp::Append { index: 0, pos: 9, value: "x".into() }), DocError::AppendPosOutOfBounds { pos: 9, line: 0, len: 3 }));
    assert!(matches!(x(DiffOp::Append { index: 0, pos: 1, value: "x\ny".into() }), DocError::AppendEol { line: 0 }));
}

/// every error says its code and the facts it has
#[test]
fn every_error_has_its_code() {
    let errors = [
        (DocError::PathNotFound { path: DocIndex::Name("k".into()) }, "DD-E001", "\"k\""),
        (DocError::NotAString { path: DocIndex::Idx(3) }, "DD-E002", "3"),
        (DocError::IndexMismatch { index: DocIndex::Name("k".into()) }, "DD-E003", "\"k\""),
        (DocError::NotAContainer { path: DocIndex::Name("k".into()) }, "DD-E004", "\"k\""),
        (DocError::NotAContainer { path: DocIndex::Idx(4) }, "DD-E005", "4"),
        (DocError::LineOutOfBounds { op: LineOp::Remove, index: 7, len: 2 }, "DD-E101", "7"),
        (DocError::LineOutOfBounds { op: LineOp::Insert, index: 7, len: 2 }, "DD-E102", "7"),
        (DocError::LineOutOfBounds { op: LineOp::Update, index: 7, len: 2 }, "DD-E103", "7"),
        (DocError::LineOutOfBounds { op: LineOp::Append, index: 7, len: 2 }, "DD-E104", "7"),
        (DocError::AppendPosOutOfBounds { pos: 8, line: 1, len: 3 }, "DD-E105", "8"),
        (DocError::AppendEol { line: 6 }, "DD-E106", "6"),
    ];
    for (e, code, fact) in errors {
        let m = e.to_string();
        assert!(m.starts_with(&format!("{} ", code)) && m.contains(fact), "{}", m);
        assert!(std::error::Error::source(&e).is_none());
    }
}

/// any patch, as one JSON for storage or the wire
#[test]
fn mismatches_display_as_json() {
    let base = doc(r#"{"a": 1}"#);
    let m = Mismatches::Doc(Mismatch::new(&base, &doc(r#"{"a": 2}"#)).unwrap());
    assert_eq!(m.to_string(), r#"{"Doc":[{"p":[{"n":"a"}],"v":{"Update":2}}]}"#);
    let t = Mismatches::Text(txt::Mismatch::new(&"a".to_string(), &"b".to_string()).unwrap());
    assert_eq!(serde_json::from_str::<Mismatches>(&t.to_string()).unwrap(), t);
}
