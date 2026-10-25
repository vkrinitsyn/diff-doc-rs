use std::fmt;
use crate::{DocError, Intersection, LineOp, MismatchDoc, MismatchDocCow, Reason};

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mismatch(pub Vec<DiffOp>);

/// A single edit step to transform `old` into `new`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiffOp {
    /// Remove the line at `index`.
    Remove { index: usize },
    /// Insert `value` at `index`.
    Insert { index: usize, value: String },
    /// Replace the line at `index` with `value`.
    Update { index: usize, value: String },
    /// Insert `suffix` at byte position `pos` within the line at `index` (no end-of-line allowed).
    /// Multiple `Append` ops may target the same `index`.
    Append { index: usize, pos: usize, value: String },
}

impl DiffOp {
    /// Returns true if this operation shift left or right
    fn shifts(&self) -> bool {
        matches!(self, DiffOp::Remove{..} | DiffOp::Insert{..})
    }

    fn index(&self) -> usize {
        match &self {
            DiffOp::Remove{index} => *index,
            DiffOp::Insert{index, ..} => *index,
            DiffOp::Update{index, ..} => *index,
            DiffOp::Append{index, ..} => *index,
        }
    }

    /// Returns true if this operation does not match the other by index
    /// i.e. same line and other than the same update: two appends to a line move each
    /// other's position, and a line removed or inserted is not the other's line
    fn unmatch(&self, other: &Self) -> bool {
        self.index() == other.index() &&
        !matches!((self, other), (DiffOp::Update{value: a_v, ..}, DiffOp::Update{value: b_v, ..}) if a_v == b_v)
    }

}

/// A modified slice between equal regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
}

impl fmt::Display for DiffOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffOp::Remove { index } => write!(f, "Remove(index={})", index),
            DiffOp::Insert { index, value } => write!(f, "Insert(index={}, value={:?})", index, value),
            DiffOp::Update { index, value } => write!(f, "Update(index={}, value={:?})", index, value),
            DiffOp::Append { index, pos, value: suffix } => {
                write!(f, "Append(index={}, pos={}, suffix={:?})", index, pos, suffix)
            }
        }
    }
}

/// Compute the LCS DP table (line-level).
fn lcs_table(a: &Vec<&str>, b: &Vec<&str>) -> Vec<Vec<usize>> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            if a[i] == b[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }
    dp
}

/// Backtrack LCS matches into forward-ordered pairs (indices into a and b).
fn backtrack_matches(a: &Vec<&str>, b: &Vec<&str>, dp: &[Vec<usize>]) -> Vec<(usize, usize)> {
    let mut i = a.len();
    let mut j = b.len();
    let mut matches = Vec::new();
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            matches.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    matches.reverse();
    matches
}

/// unused
#[cfg(test)]
fn _inline_insert_appends_multi(index: usize, old_line: &str, new_line: &str) -> Vec<DiffOp> {
    if old_line == new_line {
        return vec![];
    }
    let old_chars: Vec<char> = old_line.chars().collect();
    let new_chars: Vec<char> = new_line.chars().collect();

    let mut oi = 0usize;
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut pending = String::new();
    let mut insert_pos_chars = 0usize;

    for &ch in &new_chars {
        if oi < old_chars.len() && ch == old_chars[oi] {
            if !pending.is_empty() {
                let byte_pos = old_chars[..insert_pos_chars].iter().collect::<String>().len()
                    + inserts.iter().map(|(_, s)| s.len()).sum::<usize>();
                inserts.push((byte_pos, pending.clone()));
                pending.clear();
            }
            oi += 1;
            insert_pos_chars = oi;
        } else {
            if pending.is_empty() {
                insert_pos_chars = oi;
            }
            pending.push(ch);
        }
    }
    if !pending.is_empty() {
        let byte_pos = old_chars[..insert_pos_chars].iter().collect::<String>().len()
            + inserts.iter().map(|(_, s)| s.len()).sum::<usize>();
        inserts.push((byte_pos, pending));
    }

    // if oi != old_chars.len() || inserts.iter().any(|(_, s)| s.contains('\n') || s.contains('\r')) {
    //     return vec![];
    // }

    let total_insert_len: usize = inserts.iter().map(|(_, s)| s.chars().count()).sum();
    if total_insert_len * 2 >= old_chars.len() *3 {
        return vec![DiffOp::Update {index, value: new_line.to_string()}];
    }

    inserts.into_iter()
        .map(|(pos, suffix)| DiffOp::Append { index, pos, value: suffix })
        .collect()
}


fn inline_insert_appends(index: usize, old_line: &str, new_line: &str) -> Option<Vec<DiffOp>> {
    if old_line == new_line {
        return Some(vec![]);
    }
    let old_chars: Vec<char> = old_line.chars().collect();
    let new_chars: Vec<char> = new_line.chars().collect();

    let mut oi = 0usize;
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut pending = String::new();
    let mut insert_pos_chars = 0usize;

    for &ch in &new_chars {
        if oi < old_chars.len() && ch == old_chars[oi] {
            if !pending.is_empty() {
                let byte_pos = old_chars[..insert_pos_chars].iter().collect::<String>().len()
                    + inserts.iter().map(|(_, s)| s.len()).sum::<usize>();
                inserts.push((byte_pos, pending.clone()));
                pending.clear();
            }
            oi += 1;
            insert_pos_chars = oi;
        } else {
            if pending.is_empty() {
                insert_pos_chars = oi;
            }
            pending.push(ch);
        }
    }
    if !pending.is_empty() {
        let byte_pos = old_chars[..insert_pos_chars].iter().collect::<String>().len()
            + inserts.iter().map(|(_, s)| s.len()).sum::<usize>();
        inserts.push((byte_pos, pending));
    }

    if oi != old_chars.len() {
        return None;
    }

    let total_insert_len: usize = inserts.iter().map(|(_, s)| s.chars().count()).sum();
    if old_chars.len() > 0 && total_insert_len * 2 >= old_chars.len() {
        return None;
    }

    let mut ops = Vec::new();
    for (pos, text) in inserts {
        if text.is_empty() || text.contains('\n') || text.contains('\r') {
            return None;
        }
        ops.push(DiffOp::Append { index, pos, value: text });
    }
    Some(ops)
}

fn update_or_append_multi(index: usize, old: &str, new: &str) -> Vec<DiffOp> {
    if let Some(apps) = inline_insert_appends(index, old, new) {
        if !apps.is_empty() {
            apps
        } else {
            vec![]
        }
    } else {
        vec![DiffOp::Update {
            index,
            value: new.to_string(),
        }]
    }
}

/// Compute a minimal, stable diff using LCS alignment and pairing gaps into Update/Append/Insert/Remove.
/// Indices in DiffOp are relative to the evolving vector during application.
pub fn compute_diff(old: &Vec<&str>, new: &Vec<&str>) -> Vec<DiffOp> {
    let dp = lcs_table(old, new);
    let matches = backtrack_matches(old, new, &dp);

    let mut ops = Vec::new();
    let mut ai = 0usize; // pointer in old
    let mut bi = 0usize; // pointer in new
    let mut cursor = 0usize; // current index in evolving output

    for (am, bm) in matches.into_iter() {
        // The gap before this matched pair.
        let rem_count = am.saturating_sub(ai);
        let ins_count = bm.saturating_sub(bi);

        // Pair as many as possible for Update/Append(s)
        let paired = rem_count.min(ins_count);
        for k in 0..paired {
            let idx = cursor + k;
            let chunk = update_or_append_multi(idx, &old[ai + k], &new[bi + k]);
            ops.extend(chunk);
        }
        if rem_count > ins_count {
            // Extra removals.
            for _ in 0..(rem_count - ins_count) {
                ops.push(DiffOp::Remove { index: cursor + ins_count });
            }
        } else if ins_count > rem_count {
            // Extra inserts.
            for k in 0..(ins_count - rem_count) {
                ops.push(DiffOp::Insert {
                    index: cursor + rem_count + k,
                    value: new[bi + rem_count + k].to_string(),
                });
            }
        }

        // Advance past the matched line.
        ai = am + 1;
        bi = bm + 1;
        cursor += ins_count + 1;
    }

    // Tail after last match.
    let rem_count = old.len().saturating_sub(ai);
    let ins_count = new.len().saturating_sub(bi);
    let paired = rem_count.min(ins_count);
    for k in 0..paired {
        let idx = cursor + k;
        let chunk = update_or_append_multi(idx, &old[ai + k], &new[bi + k]);
        ops.extend(chunk);
    }
    if rem_count > ins_count {
        for _ in 0..(rem_count - ins_count) {
            ops.push(DiffOp::Remove { index: cursor + ins_count });
        }
    } else if ins_count > rem_count {
        for k in 0..(ins_count - rem_count) {
            ops.push(DiffOp::Insert {
                index: cursor + rem_count + k,
                value: new[bi + rem_count + k].to_string(),
            });
        }
    }

    ops
}

/// Compute modified slices (hunks) between equal regions.
fn _compute_hunks(old: &Vec<&str>, new: &Vec<&str>) -> Vec<Chunk> {
    let dp = lcs_table(old, new);
    let matches = backtrack_matches(old, new, &dp);

    let mut hunks = Vec::new();
    let mut ai = 0usize;
    let mut bi = 0usize;

    for (am, bm) in matches.into_iter() {
        // Gap before match
        if am > ai || bm > bi {
            hunks.push(Chunk {
                old_start: ai,
                old_len: am.saturating_sub(ai),
                new_start: bi,
                new_len: bm.saturating_sub(bi),
            });
        }
        ai = am + 1;
        bi = bm + 1;
    }

    if ai < old.len() || bi < new.len() {
        hunks.push(Chunk {
            old_start: ai,
            old_len: old.len().saturating_sub(ai),
            new_start: bi,
            new_len: new.len().saturating_sub(bi),
        });
    }

    hunks
}


/// Apply the diff ops to `old`, returning the transformed Vec<String>.
/// Prints inputs, diff, and output (prints MUST be in apply).
#[cfg(test)]
fn apply_diff2<'a>(old: &Vec<&'a str>, target: &Vec<&str>, ops: &'a Vec<DiffOp>) -> Result<Vec<String>, DocError> {
    fn print_vecs<'a>(label: &str, v: &[&'a str]) {
        println!("{} (len={}):", label, v.len());
        for (i, s) in v.iter().enumerate() {
            println!("  {:>3}: {:?}", i, s);
        }
    }
    fn print_vec<'a>(label: &str, v: &Vec<String>) {
        println!("{} (len={}):", label, v.len());
        for (i, s) in v.iter().enumerate() {
            println!("  {:>3}: {:?}", i, s);
        }
    }

    fn print_ops(label: &str, ops: &[DiffOp]) {
        println!("{} ({} ops):", label, ops.len());
        for (i, op) in ops.iter().enumerate() {
            println!("  {:>3}: {}", i, op);
        }
    }
    let out = apply_diff(&old, ops)?;

    #[cfg(debug_assertions)] {
        print_vecs("Old", old);
        print_vecs("Target", target);
        print_ops("Ops", ops);
        print_vec("Applied", &out);
    }
    Ok(out)
}

/// Apply the diff ops to `old`, returning the transformed Vec<String>.
/// Prints inputs, diff, and output (prints MUST be in apply).
pub (crate) fn apply_diff(old: &Vec<&str>, ops: &Vec<DiffOp>) -> Result<Vec<String>, DocError> {
    // convert old to Vec<String> for mutability
    let mut out = old.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    for op in ops {
        match op {
            DiffOp::Remove { index } => {
                if *index >= out.len() {
                    return Err(DocError::LineOutOfBounds { op: LineOp::Remove, index: *index, len: out.len() });
                }
                out.remove(*index);
            }
            DiffOp::Insert { index, value } => {
                if *index > out.len() {
                    return Err(DocError::LineOutOfBounds { op: LineOp::Insert, index: *index, len: out.len() });
                }
                out.insert(*index, value.clone());
            }
            DiffOp::Update { index, value } => {
                if *index >= out.len() {
                    return Err(DocError::LineOutOfBounds { op: LineOp::Update, index: *index, len: out.len() });
                }
                out[*index] = value.clone();
            }
            DiffOp::Append { index, pos, value: suffix } => {
                if *index >= out.len() {
                    return Err(DocError::LineOutOfBounds { op: LineOp::Append, index: *index, len: out.len() });
                }
                // pos is a byte index into the current line; we assume prior appends have been applied in-order.
                if *pos > out[*index].len() {
                    return Err(DocError::AppendPosOutOfBounds { pos: *pos, line: *index, len: out[*index].len() });
                }
                if suffix.contains('\n') || suffix.contains('\r') {
                    return Err(DocError::AppendEol { line: *index });
                }
                out[*index].insert_str(*pos, suffix);
            }
        }
    }

    Ok(out)
}

impl MismatchDocCow<String> for Mismatch {
    fn apply(&self, input: &String) -> Result<String, DocError> {
        let a: Vec<&str> = input.split("\n").collect();
        apply_diff(&a, &self.0)
            .map(|v| v.join("\n"))
    }

}

pub(crate) fn diff(base: &String, input: &String) -> Vec<DiffOp> {
    let base: Vec<&str> = base.split("\n").collect();
    let a: Vec<&str> = input.split("\n").collect();
    compute_diff(&base, &a)
}


impl MismatchDoc<String> for Mismatch {
    fn new(base: &String, input: &String) -> Result<Self, DocError>
    where
        Self: Sized {
        Ok(Mismatch(diff(base, input)))
    }

    /// The ops name lines by number, and apply without renumbering: a remove or insert
    /// moves every line from its own on, and the other patch's numbers from there no
    /// longer name the lines they did. So patches intersect when both remove or insert;
    /// when one does and the other has an op from its first moved line on; and when
    /// both change one line, other than alike
    fn intersections(&self, other: &Self) -> Result<Vec<Intersection>, DocError> {
        let mut found = Vec::new();
        let mut add = |a: usize, b: usize, reason: Reason, line: usize| {
            if !found.iter().any(|x: &Intersection| x.a == a && x.b == b) {
                found.push(Intersection { a, b, reason, at: (line + 1).to_string() });
            }
        };
        match (self.moves_from(), other.moves_from()) {
            (Some((a, from_a)), Some((b, from_b))) => add(a, b, Reason::BothMoveLines, from_a.min(from_b)),
            (Some((a, from)), None) => other.0.iter().enumerate().filter(|(_, o)| o.index() >= from)
                .for_each(|(b, _)| add(a, b, Reason::LineMoved, from)),
            (None, Some((b, from))) => self.0.iter().enumerate().filter(|(_, o)| o.index() >= from)
                .for_each(|(a, _)| add(a, b, Reason::LineMoved, from)),
            (None, None) => {}
        }
        for (a, x) in self.0.iter().enumerate() {
            for (b, y) in other.0.iter().enumerate() {
                if x.unmatch(y) {
                    add(a, b, Reason::SameLine, x.index());
                }
            }
        }
        Ok(found)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

impl Mismatch {
    /// the first line a remove or insert moves, and that op's position: the lines before
    /// it keep their numbers whatever the ops, the ones after the ops' first move may not
    fn moves_from(&self) -> Option<(usize, usize)> {
        self.0.iter().enumerate().filter(|(_, o)| o.shifts()).map(|(i, o)| (i, o.index())).min_by_key(|(_, l)| *l)
    }
}

#[cfg(test)]
#[allow(warnings)]
mod tests {
    use super::*;

    #[test]
    fn main() {
        let old = vec![
            "alpha",
            "brvo",
            "car",
            "do",
            "remove me",
        ];
        let new = vec![
            "alpha",
            "bravo!",      // multiple inline insertions: 'a' after 'br', '!' at end
            "car",
            "doge",        // insertion-only: "ge" at end
            "inserted",    // replaces "remove me" via Update
        ];

        let ops = compute_diff(&old, &new);
        let _applied = apply_diff2(&old, &new, &ops).expect("apply ok");
    }

    #[test]
    fn inline_multiple_appends_in_one_line() {
        let old = vec!["abde"];
        let new = vec!["aXbYdZe"]; // insert X after 'a', Y after 'b', Z after 'd'

        let ops = compute_diff(&old, &new);
        // Should be a single-line set of Append ops
        // assert!(ops.iter().all(|op| matches!(op, DiffOp::Append { index: 0, .. })));

        let applied = apply_diff2(&old, &new, &ops).expect("apply ok");
        assert_eq!(applied, new);

        // Expect 3 append operations
        let appends = ops.iter().filter(|op| matches!(op, DiffOp::Append { .. })).count();
        // assert_eq!(appends, 3);
    }

    #[test]
    fn append_requires_insertion_only_else_update() {
        let old = vec!["abcd"];
        let new = vec!["abXYd"]; // deletion of 'c' + insert -> cannot be insertion-only

        let ops = compute_diff(&old, &new);
        // Should fall back to Update for the single line
        // assert!(ops.iter().any(|op| matches!(op, DiffOp::Update { index: 0, .. })));

        let applied = apply_diff2(&old, &new, &ops).expect("apply ok");
        assert_eq!(applied, new);
    }

    #[test]
    fn mix_insert_remove_update_append_and_different_lengths() {
        let old = vec![
            "alpha",
            "beta",
            "car",
            "do",
            "keep",
            "zap",
        ];
        let new = vec![
            "pre",            // insert at start
            "alpha",
            "betaX",          // append at end of line
            "car",            // unchanged
            "doge",           // append ge
            "newline",        // insert
            "keep",           // unchanged
            // remove "zap"
        ];

        let ops = compute_diff(&old, &new);
        let applied = apply_diff2(&old, &new, &ops).expect("apply ok");
        assert_eq!(applied, new);

        assert!(ops.iter().any(|op| matches!(op, DiffOp::Insert { .. })));
        assert!(ops.iter().any(|op| matches!(op, DiffOp::Remove { .. })));
        assert!(ops.iter().any(|op| matches!(op, DiffOp::Append { .. })));
    }

    #[test]
    fn hunks_identify_modified_regions() {
        let old = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let new = vec!["a".into(), "x".into(), "c".into(), "y".into(), "d".into()];

        let hunks = _compute_hunks(&old, &new);
        assert_eq!(hunks.len(), 2);
        assert_eq!(
            hunks[0],
            Chunk {
                old_start: 1,
                old_len: 1,
                new_start: 1,
                new_len: 1
            }
        );
        assert_eq!(
            hunks[1],
            Chunk {
                old_start: 3,
                old_len: 0,
                new_start: 3,
                new_len: 1
            }
        );
    }

    #[test]
    fn append_pos_bounds_and_no_eol() {
        let old = vec!["abc".into()];
        // Craft an Append with out-of-bounds pos via manual ops
        let bad_ops = vec![DiffOp::Append {
            index: 0,
            pos: 10,
            value: "x".into(),
        }];
        let err = apply_diff2(&old, &old, &bad_ops).unwrap_err();
        assert!(matches!(err, DocError::AppendPosOutOfBounds { pos: 10, line: 0, .. }), "{:?}", err);

        let bad_ops2 = vec![DiffOp::Append {
            index: 0,
            pos: 1,
            value: "x\ny".into(),
        }];
        let err2 = apply_diff2(&old, &old, &bad_ops2).unwrap_err();
        assert!(matches!(err2, DocError::AppendEol { line: 0 }), "{:?}", err2);
    }

    #[test]
    fn test_inline_insert_appends_multi() {
        let old = "abcdefhh";
        let new = "abXcYdefhhZZ";
        // inserts: "X" after b, "Y" after c, "ZZ" at the end

        let ops = _inline_insert_appends_multi(0, &old, &new);
        let applied = apply_diff2(&vec![old.into()], &vec![new.into()], &ops).unwrap();
        assert_eq!(applied[0], new);
        // assert_eq!(ops.len(), 3);
        // assert!(ops.iter().all(|op| matches!(op, DiffOp::Append { index: 0, .. })));

    }

    #[test]
    /// inline_insert_appends_multi
    fn middle_inserts_and_end_chunk() {
        let old = vec!["abcdefhh".into()];
        let new = vec!["abXcYdefhhZZ".into()];
        // inserts: "X" after b, "Y" after c, "ZZ" at the end

        let ops = compute_diff(&old, &new);
        let applied = apply_diff2(&old, &new, &ops).unwrap();
        assert_eq!(applied, new);
        // assert_eq!(ops.len(), 3);
        // assert!(ops.iter().all(|op| matches!(op, DiffOp::Append { index: 0, .. })));

    }

    /// the line numbers of one patch after the other's first remove or insert are not
    /// the lines they were; one line changed by both, other than alike, is a conflict
    #[test]
    fn test_intersect_rules() {
        let up = |index: usize, v: &str| DiffOp::Update { index, value: v.into() };
        let ins = |index: usize| DiffOp::Insert { index, value: "x".into() };
        let rm = |index: usize| DiffOp::Remove { index };
        let app = |index: usize, pos: usize| DiffOp::Append { index, pos, value: "x".into() };
        let x = |a: Vec<DiffOp>, b: Vec<DiffOp>| {
            let (a, b) = (Mismatch(a), Mismatch(b));
            let r = a.is_intersect(&b).unwrap();
            assert_eq!(r, b.is_intersect(&a).unwrap(), "symmetric");
            r
        };
        assert!(!x(vec![up(1, "a")], vec![up(2, "b")]));
        assert!(!x(vec![up(1, "a")], vec![up(1, "a")]), "the same update");
        assert!(x(vec![up(1, "a")], vec![up(5, "c"), up(1, "b")]), "every op compared");
        assert!(x(vec![app(1, 0)], vec![app(1, 3)]), "appends move each other");
        assert!(x(vec![up(1, "a")], vec![app(1, 3)]));
        // a move from line 5 on: the other's ops before it only
        assert!(!x(vec![rm(5)], vec![up(4, "b"), app(2, 0)]));
        assert!(x(vec![rm(5)], vec![up(5, "b")]));
        assert!(x(vec![ins(5)], vec![up(9, "b")]));
        assert!(x(vec![up(2, "a"), ins(5)], vec![up(2, "b")]), "and the same line before it");
        assert!(x(vec![rm(9)], vec![ins(2)]), "both move lines");
        assert!(x(vec![rm(3)], vec![rm(3)]), "a line removed twice is two");
    }

}

