use crate::dd::enums::{DiffEntry, DiffType};
use crate::dd::enums::DiffTreeNode;

/// Structure holding the differences after a compare operation.
/// For more readable access use the [`Mismatch::all_diffs`] method that yields a [`DiffEntry`] per diff.
#[derive(Debug, PartialEq)]
pub struct Mismatch {
    pub left_only: DiffTreeNode,
    pub right_only: DiffTreeNode,
    pub unequal_values: DiffTreeNode,
}

impl Mismatch {
    pub fn new(l: DiffTreeNode, r: DiffTreeNode, u: DiffTreeNode) -> Mismatch {
        Mismatch {
            left_only: l,
            right_only: r,
            unequal_values: u,
        }
    }

    pub fn empty() -> Self {
        Mismatch {
            left_only: DiffTreeNode::Null,
            unequal_values: DiffTreeNode::Null,
            right_only: DiffTreeNode::Null,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.left_only == DiffTreeNode::Null
            && self.unequal_values == DiffTreeNode::Null
            && self.right_only == DiffTreeNode::Null
    }

    pub fn all_diffs(&self) -> Vec<(DiffType, DiffEntry)> {
        let both = self
            .unequal_values
            .get_diffs()
            .into_iter()
            .map(|k| (DiffType::Mismatch, k));
        let left = self
            .left_only
            .get_diffs()
            .into_iter()
            .map(|k| (DiffType::LeftExtra, k));
        let right = self
            .right_only
            .get_diffs()
            .into_iter()
            .map(|k| (DiffType::RightExtra, k));

        both.chain(left).chain(right).collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn empty_diffs() {
        let empty = Mismatch::empty();
        let all_diffs = empty.all_diffs();
        assert!(all_diffs.is_empty());
    }
}
