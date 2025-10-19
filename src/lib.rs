#[cfg(feature="patch")] pub mod patch;
pub mod txt;
pub mod diff;
pub mod generic;
mod vec_processor;
mod map_processor;

use std::borrow::Cow;
use std::fmt;
use std::fmt::Display;

/// document were implements contract to deal with differences
pub trait MismatchDoc<T> {
    fn new(base: &T, input: &T) -> Result<Self, DocError> where Self: Sized;

    fn is_intersect(&self, other: &Self) -> Result<bool, DocError>;

    fn len(&self) -> usize;
}

/// document update as mutation
pub trait MismatchDocMut<T> {
    
    /// if fail_fast then stop on first error, 
    /// otherwise try to apply all and collect errors on Ok response 
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


#[derive(Debug)]
pub struct DocError(Cow<'static, str>);

impl DocError {
    fn new<E: Into<Cow<'static, str>>>(e: E) -> Self {
        Self(e.into())
    }
}
impl std::error::Error for DocError {}

impl Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
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