//! XML as a GenericValue, and back: generic.rs's from_xml and to_xml.
//!
//! XML has no maps, arrays or numbers of its own, so the mapping is this crate's:
//! - the document: a map of one key, its root element's name
//! - an element: a map of its attributes, as `@name`, and its child elements by name;
//!   an attribute's value is read as an element's text is
//!   a name repeated is an array of them, in their order
//! - an element of text only: the text - a number when it reads as one, as JSON's
//!   strings do (generic.rs) - and of nothing at all: null
//! - text beside child elements: `$text`, trimmed
//! - a namespace declared on an element: its attribute, `@xmlns` or `@xmlns:prefix`;
//!   a name in a namespace: `prefix:name`, as written
//!
//! Not kept: comments, processing instructions, the DOCTYPE, and the order of siblings
//! of different names when they interleave (`<a/><b/><a/>` writes back as `<a/><a/><b/>`).
//! One element of a name is its value, two an array: adding the second one changes the
//! type, as the diff shows.

use std::collections::HashMap;
use std::fmt::Write;

use crate::generic::{is_numeric, GenericValue, NumericString};
use crate::DocError;

/// the text of an element: a number when it reads as one
fn text(t: &str) -> GenericValue {
    if is_numeric(t) {
        GenericValue::Numeric(NumericString(t.to_string()))
    } else {
        GenericValue::StringValue(t.to_string())
    }
}

/// a name as written: with its namespace's prefix, when it has one
fn qualified(node: roxmltree::Node, namespace: Option<&str>, name: &str) -> String {
    match namespace.and_then(|uri| node.lookup_prefix(uri)) {
        Some(prefix) if !prefix.is_empty() => format!("{}:{}", prefix, name),
        _ => name.to_string(),
    }
}

fn element(node: roxmltree::Node) -> GenericValue {
    let mut map = HashMap::new();
    // the namespaces declared here: an element has its parent's too
    let parent: Vec<_> = node.parent_element().map(|p| p.namespaces().collect()).unwrap_or_default();
    for ns in node.namespaces().filter(|ns| !parent.contains(ns)) {
        let key = ns.name().map_or("@xmlns".to_string(), |p| format!("@xmlns:{}", p));
        map.insert(key, GenericValue::StringValue(ns.uri().to_string()));
    }
    for a in node.attributes() {
        map.insert(format!("@{}", qualified(node, a.namespace(), a.name())), text(a.value()));
    }
    let mut children: Vec<(String, Vec<GenericValue>)> = Vec::new();
    let mut body = String::new();
    for c in node.children() {
        if c.is_element() {
            let name = qualified(c, c.tag_name().namespace(), c.tag_name().name());
            let value = element(c);
            match children.iter_mut().find(|(n, _)| *n == name) {
                Some((_, values)) => values.push(value),
                None => children.push((name, vec![value])),
            }
        } else if let Some(t) = c.text() {
            body.push_str(t);
        }
    }
    if map.is_empty() && children.is_empty() {
        return if body.is_empty() { GenericValue::Null } else { text(&body) };
    }
    if !body.trim().is_empty() {
        map.insert("$text".to_string(), text(body.trim()));
    }
    for (name, mut values) in children {
        let v = if values.len() == 1 { values.remove(0) } else { GenericValue::Array(values) };
        map.insert(name, v);
    }
    GenericValue::Map(map)
}

pub(crate) fn from_xml(s: &str) -> Result<GenericValue, roxmltree::Error> {
    let doc = roxmltree::Document::parse(s)?;
    let root = doc.root_element();
    let name = qualified(root, root.tag_name().namespace(), root.tag_name().name());
    Ok(GenericValue::Map(HashMap::from([(name, element(root))])))
}

/// an XML name: a letter, `_` or `:` first, then those, digits, `-` and `.`
fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == ':')
        && chars.all(|c| c.is_alphanumeric() || "_:-.".contains(c))
}

fn escape(s: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// a single value as text: null as empty, a map or an array as none
fn scalar(v: &GenericValue) -> Option<String> {
    match v {
        GenericValue::StringValue(s) => Some(s.clone()),
        GenericValue::Numeric(n) => Some(n.0.clone()),
        GenericValue::Boolean(b) => Some(b.to_string()),
        GenericValue::Null => Some(String::new()),
        GenericValue::Map(_) | GenericValue::Array(_) => None,
    }
}

/// `name` with `value`, at `depth` of indentation; `at` is the path of names to it
fn write_element(out: &mut String, name: &str, value: &GenericValue, depth: usize, at: &str) -> Result<(), DocError> {
    let at = if at.is_empty() { name.to_string() } else { format!("{}/{}", at, name) };
    if !is_name(name) {
        return Err(DocError::XmlName { name: name.to_string(), at });
    }
    let indent = "  ".repeat(depth);
    match value {
        GenericValue::Array(items) => {
            for item in items {
                if matches!(item, GenericValue::Array(_)) {
                    return Err(DocError::XmlArrayInArray { at });
                }
                // each item an element of the same name, at the same path
                write_element(out, name, item, depth, at.rsplit_once('/').map_or("", |(p, _)| p))?;
            }
        }
        GenericValue::Map(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let _ = write!(out, "{}<{}", indent, name);
            for k in keys.iter().filter(|k| k.starts_with('@')) {
                let a = &k[1..];
                let v = scalar(&map[*k]).filter(|_| is_name(a))
                    .ok_or_else(|| DocError::XmlAttribute { name: a.to_string(), at: at.clone() })?;
                let _ = write!(out, " {}=\"{}\"", a, escape(&v, true));
            }
            let children: Vec<&&String> = keys.iter().filter(|k| !k.starts_with('@') && k.as_str() != "$text").collect();
            let body = map.get("$text").and_then(scalar).filter(|t| !t.is_empty());
            if children.is_empty() && body.is_none() {
                out.push_str("/>\n");
                return Ok(());
            }
            out.push('>');
            if let Some(t) = &body {
                out.push_str(&escape(t, false));
            }
            if children.is_empty() {
                let _ = writeln!(out, "</{}>", name);
                return Ok(());
            }
            out.push('\n');
            for k in children {
                write_element(out, k, &map[*k], depth + 1, &at)?;
            }
            let _ = writeln!(out, "{}</{}>", indent, name);
        }
        GenericValue::Null => {
            let _ = writeln!(out, "{}<{}/>", indent, name);
        }
        v => {
            let t = scalar(v).unwrap_or_default();
            let _ = writeln!(out, "{}<{}>{}</{}>", indent, name, escape(&t, false), name);
        }
    }
    Ok(())
}

pub(crate) fn to_xml(value: &GenericValue) -> Result<String, DocError> {
    match value {
        GenericValue::Map(m) if m.len() == 1 => {
            let (name, root) = m.iter().next().unwrap();
            if matches!(root, GenericValue::Array(_)) {
                return Err(DocError::XmlRoot);
            }
            let mut out = String::new();
            write_element(&mut out, name, root, 0, "")?;
            Ok(out)
        }
        _ => Err(DocError::XmlRoot),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generic::from_json;

    fn json(s: &str) -> GenericValue {
        from_json(s).unwrap()
    }

    /// the mapping: attributes, repeated names, text, numbers, empty, mixed text
    #[test]
    fn xml_as_a_generic_value() {
        let v = from_xml(r#"<svc id="7"><name>api</name><port>8080</port><tag>a</tag><tag>b</tag><none/>
            <note lang="en">hi <b>there</b></note></svc>"#).unwrap();
        assert_eq!(v, json(r#"{"svc": {"@id": "7", "name": "api", "port": 8080, "tag": ["a", "b"], "none": null,
            "note": {"@lang": "en", "$text": "hi", "b": "there"}}}"#));
    }

    /// what to_xml writes, from_xml reads back the same
    #[test]
    fn written_reads_back() {
        let v = from_xml(r#"<svc id="7"><name>a &amp; b</name><tag>a</tag><tag>b</tag><none/></svc>"#).unwrap();
        let x = to_xml(&v).unwrap();
        assert_eq!(x, "<svc id=\"7\">\n  <name>a &amp; b</name>\n  <none/>\n  <tag>a</tag>\n  <tag>b</tag>\n</svc>\n");
        assert_eq!(from_xml(&x).unwrap(), v);
    }

    /// namespaces: declared ones as attributes, names with their prefix
    #[test]
    fn namespaces() {
        let s = r#"<p:doc xmlns:p="urn:p" xmlns="urn:d"><p:item p:k="v">x</p:item><plain>y</plain></p:doc>"#;
        let v = from_xml(s).unwrap();
        assert_eq!(v, json(r#"{"p:doc": {"@xmlns:p": "urn:p", "@xmlns": "urn:d",
            "p:item": {"@p:k": "v", "$text": "x"}, "plain": "y"}}"#));
        assert_eq!(from_xml(&to_xml(&v).unwrap()).unwrap(), v);
    }

    /// values XML cannot hold
    #[test]
    fn not_xml() {
        let x = |s: &str| to_xml(&json(s)).unwrap_err();
        assert!(matches!(x(r#"{"a": 1, "b": 2}"#), DocError::XmlRoot));
        assert!(matches!(x("[1]"), DocError::XmlRoot));
        assert!(matches!(x(r#"{"a": [1, 2]}"#), DocError::XmlRoot));
        assert!(matches!(x(r#"{"a": {"1x": 1}}"#), DocError::XmlName { name, at } if name == "1x" && at == "a/1x"));
        assert!(matches!(x(r#"{"a": {"l": [[1]]}}"#), DocError::XmlArrayInArray { at } if at == "a/l"));
        assert!(matches!(x(r#"{"a": {"@k": [1]}}"#), DocError::XmlAttribute { name, at } if name == "k" && at == "a"));
        for e in [x(r#"{"a": 1, "b": 2}"#), x(r#"{"a": {"1x": 1}}"#), x(r#"{"a": {"l": [[1]]}}"#), x(r#"{"a": {"@k": [1]}}"#)] {
            assert!(e.to_string().starts_with("DD-E40"), "{}", e);
        }
        // booleans and numbers as text
        assert_eq!(to_xml(&json(r#"{"a": {"t": true, "n": 1.5}}"#)).unwrap(), "<a>\n  <n>1.5</n>\n  <t>true</t>\n</a>\n");
    }
}
