use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use serde_json::Value;
use thiserror::Error;
use vg_errortools::FatIOError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Misc error: {0}")]
    Misc(String),
    #[error("Error opening file: {0}")]
    IOError(#[from] FatIOError),
    #[error("Error parsing first json: {0}")]
    JSON(#[from] serde_json::Error),
    #[error("Regex compilation error: {0}")]
    Regex(#[from] regex::Error),
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Misc(value)
    }
}

#[derive(Debug, PartialEq)]
pub enum DiffTreeNode {
    Null,
    /// mismatched values left vs right
    Value(Value, Value),
    /// new value only
    Value1(Value),
    Node(HashMap<String, DiffTreeNode>),
    Array(Vec<(usize, DiffTreeNode)>),
}

impl<'a> DiffTreeNode {
    pub fn get_diffs(&'a self) -> Vec<DiffEntry<'a>> {
        let mut buf = Vec::new();
        self.follow_path(&mut buf, &[]);
        buf
    }

    pub fn follow_path<'b>(
        &'a self,
        diffs: &mut Vec<DiffEntry<'a>>,
        offset: &'b [PathElement<'a>],
    ) {
        match self {
            DiffTreeNode::Null => {
                let is_map_child = offset
                    .last()
                    .map(|o| matches!(o, PathElement::Object(_)))
                    .unwrap_or_default();
                if is_map_child {
                    diffs.push(DiffEntry {
                        path: offset.to_vec(),
                        old_value: None,
                        value: None,
                    });
                }
            }
            DiffTreeNode::Value(l, r) => diffs.push(DiffEntry {
                path: offset.to_vec(),
                old_value: Some(l),
                value: Some(r),
            }),
            DiffTreeNode::Value1(v) =>diffs.push(DiffEntry {
                path: offset.to_vec(),
                old_value: None,
                value: Some(v),
            }),
            DiffTreeNode::Node(o) => {
                for (k, v) in o {
                    let mut new_offset = offset.to_vec();
                    new_offset.push(PathElement::Object(k));
                    v.follow_path(diffs, &new_offset);
                }
            }
            DiffTreeNode::Array(v) => {
                for (l, k) in v {
                    let mut new_offset = offset.to_vec();
                    new_offset.push(PathElement::ArrayEntry(*l));
                    k.follow_path(diffs, &new_offset);
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum DiffType {
    RootMismatch,
    LeftExtra,
    RightExtra,
    Mismatch,
}

impl Display for DiffType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            DiffType::RootMismatch => "Mismatch at root.",
            DiffType::LeftExtra => "Extra on left",
            DiffType::RightExtra => "Extra on right",
            DiffType::Mismatch => "Mismatched",
        };
        write!(f, "{}", msg)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathElement<'a> {
    Object(&'a str),
    ArrayEntry(usize),
}

impl<'a> PathElement<'a> {
    pub fn resolve<'b>(&self, v: &'b serde_json::Value) -> Option<&'b serde_json::Value> {
        match self {
            PathElement::Object(o) => v.get(o),
            PathElement::ArrayEntry(i) => v.get(*i),
        }
    }

    pub fn resolve_mut<'b>(
        &self,
        v: &'b mut serde_json::Value,
    ) -> Option<&'b mut serde_json::Value> {
        match self {
            PathElement::Object(o) => v.get_mut(o),
            PathElement::ArrayEntry(i) => v.get_mut(*i),
        }
    }
}

/// A view on a single end-node of the [`DiffTreeNode`] tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffEntry<'a> {
    pub path: Vec<PathElement<'a>>,
    // pub values: Option<(&'a serde_json::Value, &'a serde_json::Value)>,
    /// left or previous value
    pub old_value: Option<&'a serde_json::Value>,
    /// right, or left (old) of right not exist 
    pub value: Option<&'a serde_json::Value>,
}

impl<'a> DiffEntry<'a> {
    pub fn resolve<'b>(&'a self, value: &'b serde_json::Value) -> Option<&'b serde_json::Value> {
        let mut return_value = value;
        for a in &self.path {
            return_value = a.resolve(return_value)?;
        }
        Some(return_value)
    }
}

impl Display for DiffEntry<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for element in &self.path {
            write!(f, ".{element}")?;
        }
        /*
        if let Some((l, r)) = &self.values {
            if l != r {
                write!(f, ".({l} != {r})")?;
            } else {
                write!(f, ".({l})")?;
            }
        }
        */
        Ok(())
    }
}

impl Display for PathElement<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PathElement::Object(o) => {
                write!(f, "{o}")
            }
            PathElement::ArrayEntry(l) => {
                write!(f, "[{l}]")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::dd::process::match_json;
    use crate::dd::sort::sort_value;

    #[test]
    fn test_resolve() {
        let data1 = json! {["a",{"c": ["d","f"] },"b"]};
        let data2 = json! {["b",{"c": ["e","d"] },"a"]};
        let diffs = match_json(&data1, &data2, true, &[]).unwrap();
        assert!(!diffs.is_empty());
        let data1_sorted = sort_value(&data1, &[]);
        let data2_sorted = sort_value(&data2, &[]);

        let all_diffs = diffs.all_diffs();
        assert_eq!(all_diffs.len(), 1);
        let (_type, diff) = all_diffs.first().unwrap();
        let val = diff.resolve(&data1_sorted);
        assert_eq!(val.unwrap().as_str().unwrap(), "f");
        let val = diff.resolve(&data2_sorted);
        assert_eq!(val.unwrap().as_str().unwrap(), "e");
    }
}
