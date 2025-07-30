pub mod patch;
pub mod txt;
pub mod json;

use std::borrow::Cow;
use std::fmt;
use std::fmt::{Display, Formatter};
use diffy::{create_patch, Patch};

/// document were implements contract to deal with differences
pub trait MismatchDoc<T> {
    fn new(base: &T, input: &T) -> Result<Self, DocError> where Self: Sized;

    fn apply_to(&self, input: &T) -> Result<T, DocError>;

    fn is_intersect(&self, other: &Self) -> Result<bool, DocError>;
}

/// supported type of document
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Mismatches {
    /// with initial diff patch content
    Patch(patch::Mismatch),
    /// json document
    Json(json::Mismatch),
    /// line number strings update and trim only
    Text(txt::Mismatch),
    // Xml document, must start with < and end with > // todo define xml document portion value
    // Xml(HashMap<String, Option<String>>),
    // yaml document: better start with --- to identify as yaml // todo define xml document portion value
    // Yaml(HashMap<String, Option<String>>),
}


/*
    /// compare two documents
    pub fn new_guess(base_a: &String, base_b: &String) -> Result<Self, DocParseError> {
        let b = Self::guess_doc_format(base_b);
        match Self::guess_doc_format(base_a) {
            Mismatch::Patch(_) =>  match &b {
                Mismatch::Patch(_) => Ok(Self { mismatch_type: Mismatch::Patch(create_patch(base_a, base_b).to_string()), diff: HashMap::new() }),
                _ => Err(DocParseError::new(format!("unmatched types: Txt and {}", b)))
            }
            Mismatch::Json => match &b {
                Mismatch::Json => Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Json and {}", b)))
            }
            Mismatch::Text => match &b {
                Mismatch::Text => Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Json and {}", b)))
            }

            Mismatch::Xml =>  match &b {
                Mismatch::Xml =>  Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Xml and {}", b)))
            }
            Mismatch::Yaml =>  match &b {
                Mismatch::Yaml =>  Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Yaml and {}", b)))
            }

        }
    }
*/


#[derive(Debug)]
pub struct DocError(Cow<'static, str>);

impl DocError {
    fn new<E: Into<Cow<'static, str>>>(e: E) -> Self {
        Self(e.into())
    }
}
impl std::error::Error for DocError {}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Mismatches {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match serde_json::to_string(&self) {
            Ok(json) => write!(f, "{}", json),
            Err(err) => write!(f, "ERR: {}", err)
        }

    }
}

mod tests {
    // #[tokio::test]
    #[test]
    fn test_compile() {
        assert!(true);
    }
}