//! Where two patches intersect: each pair of their changes that does not commute -
//! Base + A + B would not be Base + B + A - and why. MismatchDoc::intersections.

use std::fmt;

/// One change of patch A and one of patch B that do not commute
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intersection {
    /// the change of A: its position in A's hunks or ops, 0-based
    pub a: usize,
    /// the change of B: its position in B's
    pub b: usize,
    pub reason: Reason,
    /// where: a document's path as a JSON pointer (`/list/2`), a text's line (1-based),
    /// a GNU patch's lines of the base (`3-9`)
    pub at: String,
}

/// Why two changes do not commute
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reason {
    /// a document's node changed - removed, updated - by one, and inside it by the other
    Inside,
    /// a swap or clone reads an element the other changes
    OtherElement,
    /// both change one node, differently
    SameNode,
    /// an array element removed or inserted moves the other's element in that array
    ArrayShift,
    /// one takes a node as a map, the other as an array
    NodeKind,
    /// both texts remove or insert lines: each moves the other's
    BothMoveLines,
    /// a text's line removed or inserted moves the other's line, from `at` on
    LineMoved,
    /// both change one line of a text, differently
    SameLine,
    /// two hunks of GNU patches read the same lines: each needs them as they were
    HunksOverlap,
}

impl fmt::Display for Intersection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (a, b, at) = (self.a + 1, self.b + 1, &self.at);
        let m = match self.reason {
            Reason::Inside => tr!("intersect.inside", a = a, b = b, at = at),
            Reason::OtherElement => tr!("intersect.other_element", a = a, b = b, at = at),
            Reason::SameNode => tr!("intersect.same_node", a = a, b = b, at = at),
            Reason::ArrayShift => tr!("intersect.array_shift", a = a, b = b, at = at),
            Reason::NodeKind => tr!("intersect.node_kind", a = a, b = b, at = at),
            Reason::BothMoveLines => tr!("intersect.both_move_lines", a = a, b = b, at = at),
            Reason::LineMoved => tr!("intersect.line_moved", a = a, b = b, at = at),
            Reason::SameLine => tr!("intersect.same_line", a = a, b = b, at = at),
            Reason::HunksOverlap => tr!("intersect.hunks_overlap", a = a, b = b, at = at),
        };
        f.write_str(&m)
    }
}
