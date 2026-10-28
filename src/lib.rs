#![forbid(unsafe_code)]
// [i18n] locales/*.yml, compiled in; a language without a message is English
#[cfg(feature = "i18n")]
rust_i18n::i18n!("locales", fallback = "en");

#[macro_use]
mod i18n;
mod error;
mod intersection;
#[cfg(feature="patch")] pub mod patch;
pub mod txt;
pub mod diff;
pub mod generic;
mod vec_processor;
mod map_processor;
mod xml;
#[cfg(feature="bson")] mod bson_doc;

use std::fmt;
use std::fmt::Display;

pub use error::{DocError, Format, LineOp};
pub use intersection::{Intersection, Reason};
pub use i18n::set_lang;
/// the bson crate of from_bson_document and to_bson_document: its Document, `doc!`
#[cfg(feature="bson")] pub use bson;

/// document were implements contract to deal with differences
pub trait MismatchDoc<T> {
    fn new(base: &T, input: &T) -> Result<Self, DocError> where Self: Sized;

    /// where the two patches intersect: each pair of a change of self (A) and one of
    /// other (B) that do not commute, and why; empty when they are disjoint, and apply
    /// in any order to the same result
    fn intersections(&self, other: &Self) -> Result<Vec<Intersection>, DocError>;

    /// return false if no intersection between two patches and they can apply in any order
    fn is_intersect(&self, other: &Self) -> Result<bool, DocError> {
        Ok(!self.intersections(other)?.is_empty())
    }

    fn len(&self) -> usize;
}

/// document update as mutation
pub trait MismatchDocMut<T> {
    
    /// if fail_fast then stop on first error, 
    /// otherwise try to apply all and collect errors on Ok response
    /// - non-transactional behavioral, partial updates possible either on fail_fast!
    fn apply_mut(&self, input: &mut T, fail_fast: bool) -> Result<Vec<DocError>, DocError>;
}

/// document update Copy on Write
pub trait MismatchDocCow<T> {
    fn apply(&self, input: &T) -> Result<T, DocError>;
}

/// supported type of document
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Mismatches {
    #[cfg(feature="patch")]
    /// with initial diff patch content from GNU patch file
    Patch(patch::Mismatch),
    /// json or other document
    Doc(diff::Mismatch),
    /// line number strings update and trim only
    Text(txt::Mismatch),
}

impl Display for Mismatches {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match serde_json::to_string(&self) {
            Ok(json) => write!(f, "{}", json),
            Err(err) => write!(f, "ERR: {}", err)
        }

    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_compile() {
        assert!(true);
    }
}