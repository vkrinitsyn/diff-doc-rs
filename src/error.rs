//! What applying, comparing or parsing a document failed on.
//!
//! One type for the whole crate: each variant keeps the facts of its failure - the
//! path, the index, the native error - and says it, through Display, in the language
//! of [crate::set_lang] (i18n.rs). Each of diff_doc's own starts with its code, `DD-Ennn`, the
//! same in every language, so it can be looked up whatever the language.

use std::fmt;

use crate::generic::DocIndex;

/// What failed
#[derive(Debug)]
#[non_exhaustive]
pub enum DocError {
    /// no node at this path of the document
    PathNotFound { path: DocIndex },
    /// a text change (UpdateTxt) on a node that is not a string
    NotAString { path: DocIndex },
    /// a Swap or Clone with the other kind of index than the node's: a map takes a
    /// name, an array a number
    IndexMismatch { index: DocIndex },
    /// the path goes on through a node that is not a map (by name) or not an array
    /// (by number)
    NotAContainer { path: DocIndex },
    /// a line change on a line the text does not have
    LineOutOfBounds { op: LineOp, index: usize, len: usize },
    /// an append past the end of its line
    AppendPosOutOfBounds { pos: usize, line: usize, len: usize },
    /// an append that would add a line
    AppendEol { line: usize },
    /// a GNU patch that does not parse
    #[cfg(feature = "patch")]
    PatchParse(diffy::ParsePatchError),
    /// a hunk of a GNU patch, 1-based, whose lines are not in the text, nor its result
    #[cfg(feature = "patch")]
    PatchHunk { hunk: usize },
    /// XML has one root element: a map of one key, its value not an array
    XmlRoot,
    /// a key that is no XML name, at the path of element names to it
    XmlName { name: String, at: String },
    /// an array in an array: XML repeats an element, it does not nest the repetitions
    XmlArrayInArray { at: String },
    /// an attribute, `@name`, of a map or an array: an attribute has one value
    XmlAttribute { name: String, at: String },
    /// a document that does not parse or serialize, with its format's native error
    Format { format: Format, err: Box<dyn std::error::Error + Send + Sync> },
}

/// The change to a line of a text, as the text diff makes it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOp {
    Remove,
    Insert,
    Update,
    Append,
}

/// A document format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Yaml,
    Toml,
    Xml,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Json => "JSON",
            Format::Yaml => "YAML",
            Format::Toml => "TOML",
            Format::Xml => "XML",
        })
    }
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = match self {
            DocError::PathNotFound { path } => tr!("doc.path_not_found", path = index(path)),
            DocError::NotAString { path } => tr!("doc.not_a_string", path = index(path)),
            DocError::IndexMismatch { index: i } => tr!("doc.index_mismatch", index = index(i)),
            DocError::NotAContainer { path: p @ DocIndex::Name(_) } => tr!("doc.not_a_map", path = index(p)),
            DocError::NotAContainer { path: p @ DocIndex::Idx(_) } => tr!("doc.not_an_array", path = index(p)),
            DocError::LineOutOfBounds { op, index, len } => match op {
                LineOp::Remove => tr!("txt.remove_out_of_bounds", index = index, len = len),
                LineOp::Insert => tr!("txt.insert_out_of_bounds", index = index, len = len),
                LineOp::Update => tr!("txt.update_out_of_bounds", index = index, len = len),
                LineOp::Append => tr!("txt.append_out_of_bounds", index = index, len = len),
            },
            DocError::AppendPosOutOfBounds { pos, line, len } => tr!("txt.append_pos_out_of_bounds", pos = pos, line = line, len = len),
            DocError::AppendEol { line } => tr!("txt.append_eol", line = line),
            #[cfg(feature = "patch")]
            DocError::PatchParse(e) => tr!("patch.parse", err = e),
            #[cfg(feature = "patch")]
            DocError::PatchHunk { hunk } => tr!("patch.apply", hunk = hunk),
            DocError::XmlRoot => tr!("xml.root"),
            DocError::XmlName { name, at } => tr!("xml.name", name = name, at = at),
            DocError::XmlArrayInArray { at } => tr!("xml.array_in_array", at = at),
            DocError::XmlAttribute { name, at } => tr!("xml.attribute", name = name, at = at),
            // the parser's own message, as it says it: nothing of diff_doc's to translate
            DocError::Format { format, err } => format!("{}: {}", format, err),
        };
        f.write_str(&m)
    }
}

/// a path's index as the messages show it: a name in quotes, a number as it is
fn index(i: &DocIndex) -> String {
    match i {
        DocIndex::Name(n) => format!("\"{}\"", n),
        DocIndex::Idx(i) => i.to_string(),
    }
}

impl std::error::Error for DocError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(feature = "patch")]
            DocError::PatchParse(e) => Some(e),
            DocError::Format { err, .. } => Some(err.as_ref()),
            _ => None,
        }
    }
}

#[cfg(feature = "patch")]
impl From<diffy::ParsePatchError> for DocError {
    fn from(e: diffy::ParsePatchError) -> Self {
        DocError::PatchParse(e)
    }
}

/// the format errors of generic.rs's from_*/to_*, so `?` makes them a DocError
macro_rules! from_format {
    ($($t:ty => $f:ident),* $(,)?) => {$(
        impl From<$t> for DocError {
            fn from(e: $t) -> Self {
                DocError::Format { format: Format::$f, err: Box::new(e) }
            }
        }
    )*};
}

from_format! {
    serde_json::Error => Json,
    serde_saphyr::Error => Yaml,
    serde_saphyr::SerializeError => Yaml,
    toml::de::Error => Toml,
    toml::ser::Error => Toml,
    roxmltree::Error => Xml,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the facts of a failure are in its message, after its code
    #[test]
    fn a_message_has_its_code_and_facts() {
        let m = DocError::LineOutOfBounds { op: LineOp::Insert, index: 7, len: 3 }.to_string();
        assert!(m.starts_with("DD-E102 ") && m.contains('7') && m.contains('3'), "{}", m);
        let m = DocError::PathNotFound { path: DocIndex::Name("a".into()) }.to_string();
        assert!(m.starts_with("DD-E001 ") && m.contains("\"a\""), "{}", m);
        let m = DocError::NotAContainer { path: DocIndex::Idx(2) }.to_string();
        assert!(m.starts_with("DD-E005 "), "{}", m);
    }

    /// a format error keeps the native one as its source
    #[test]
    fn a_format_error_keeps_its_cause() {
        let e: DocError = crate::generic::from_json("{").unwrap_err().into();
        assert!(matches!(e, DocError::Format { format: Format::Json, .. }));
        assert!(e.to_string().starts_with("JSON: "), "{}", e);
        assert!(std::error::Error::source(&e).is_some());
    }
}
