use std::cmp::min;
use diffy::{create_patch, Hunk, Line, Patch};
use crate::{DocError, Intersection, MismatchDoc, MismatchDocCow, Reason};

/// wrapper to diffy patches with intersect calculation
///  - Text file format as https://en.wikipedia.org/wiki/Diff
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mismatch (String); // Patch

impl MismatchDoc<String> for Mismatch {
    fn new(base: &String, input: &String) -> Result<Self, DocError>
    where
        Self: Sized
    {
        Ok(Mismatch(create_patch(base, input).to_string()))
    }


    fn intersections(&self, input: &Self) -> Result<Vec<Intersection>, DocError> {
        Ok(intersect_patch(
            &Patch::from_str(self.0.as_str())?,
            &Patch::from_str(input.0.as_str())?))
    }

    fn len(&self) -> usize {
        Patch::from_str(self.0.as_str()).map(|p| p.hunks().len()).unwrap_or(0)
    }
}

impl MismatchDocCow<String> for Mismatch {
    /// Each hunk where its lines of the base are, as diffy::apply does: its context and
    /// deleted lines, exactly, searched from where the hunk says outwards. A hunk whose
    /// result is there instead is already applied, and skipped: so the same change in
    /// two patches applies once, whatever their order (is_intersect_patch)
    fn apply(&self, base: &String) -> Result<String, DocError> {
        let patch = Patch::from_str(self.0.as_str())?;
        // each line with its end of line, and whether a hunk of this patch wrote it
        let mut image: Vec<(&str, bool)> = base.split_inclusive('\n').map(|l| (l, false)).collect();
        for (i, h) in patch.hunks().iter().enumerate() {
            let at = h.new_range().start().saturating_sub(1);
            let (pre, post) = images(h);
            if let Some(pos) = find(&image, &pre, at) {
                image.splice(pos..pos + pre.len(), post.into_iter().map(|l| (l, true)));
            } else if post.is_empty() || find(&image, &post, at).is_none() {
                return Err(DocError::PatchHunk { hunk: i + 1 });
            }
        }
        Ok(image.into_iter().map(|(l, _)| l).collect())
    }
}

/// a hunk's lines before - context and deleted - and after - context and inserted
fn images<'a>(h: &Hunk<'a, str>) -> (Vec<&'a str>, Vec<&'a str>) {
    let mut pre = Vec::new();
    let mut post = Vec::new();
    for l in h.lines() {
        match l {
            Line::Context(l) => {
                pre.push(*l);
                post.push(*l);
            }
            Line::Delete(l) => pre.push(*l),
            Line::Insert(l) => post.push(*l),
        }
    }
    (pre, post)
}

/// where `lines` are in the image, none of them written by this patch yet: at `at`,
/// else the nearest, one line before then one after
fn find(image: &[(&str, bool)], lines: &[&str], at: usize) -> Option<usize> {
    let at = min(at, image.len());
    let fits = |pos: usize| image.get(pos..pos + lines.len())
        .is_some_and(|i| i.iter().map(|(l, patched)| (!patched).then_some(*l)).eq(lines.iter().map(|l| Some(*l))));
    (0..=image.len())
        .flat_map(|d| [at.checked_sub(d), (d > 0).then_some(at + d)])
        .flatten()
        .find(|&pos| fits(pos))
}

/// the lines of the base a hunk reads, context included, 0-based [start, end); with no
/// lines to read - an insertion into an empty base - the point it inserts at
fn span(h: &Hunk<str>) -> (usize, usize) {
    let r = h.old_range();
    match r.len() {
        0 => (r.start(), r.start()),
        len => (r.start() - 1, r.start() - 1 + len),
    }
}

/// two hunks read the same lines of the base, or insert at the same point
fn overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    if a.0 == a.1 || b.0 == b.1 {
        a.0 <= b.1 && b.0 <= a.1
    } else {
        a.0 < b.1 && b.0 < a.1
    }
}

/// the intersections i.e. unable to implement commutative for two patches: each pair of
/// hunks, at the lines of the base both read (1-based, `from-to`)
///
/// A hunk applies only where its context and deleted lines are, unchanged: so two
/// patches commute when no hunk of one reads a line of the base - its context
/// included - that a hunk of the other reads, as then neither changes a line the other
/// needs. The same change in both - a hunk of the same lines at the same place, same
/// lines deleted among them - is no intersection: apply skips a hunk already applied.
///
/// Not seen here, as a patch has only the lines its hunks read: a base with the lines
/// of a hunk twice, close by, where a shift made by the other patch moves the hunk to
/// the other copy.
fn intersect_patch(patch_a: &Patch<str>, patch_b: &Patch<str>) -> Vec<Intersection> {
    let mut found = Vec::new();
    for (i, a) in patch_a.hunks().iter().enumerate() {
        for (j, b) in patch_b.hunks().iter().enumerate() {
            let (sa, sb) = (span(a), span(b));
            if overlap(sa, sb) && !(a.old_range() == b.old_range() && a.lines() == b.lines()) {
                let (from, to) = (sa.0.max(sb.0), sa.1.min(sb.1));
                let at = if to > from { format!("{}-{}", from + 1, to) } else { (from + 1).to_string() };
                found.push(Intersection { a: i, b: j, reason: Reason::HunksOverlap, at });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(n: usize) -> String {
        (1..=n).map(|i| format!("line {}\n", i)).collect()
    }

    /// `text` with line `n` (1-based) replaced by `with`
    fn change(text: &str, n: usize, with: &str) -> String {
        text.lines().enumerate().map(|(i, l)| format!("{}\n", if i + 1 == n { with } else { l })).collect()
    }

    fn mismatch(base: &str, input: &str) -> Mismatch {
        Mismatch::new(&base.to_string(), &input.to_string()).unwrap()
    }

    /// apply gives what diffy::apply gives
    #[test]
    fn apply_is_diffy_apply() {
        let base = lines(30);
        let input = change(&change(&base, 3, "three"), 25, "twenty five").replace("line 12\n", "");
        let m = mismatch(&base, &input);
        assert_eq!(m.len(), 3);
        assert_eq!(m.apply(&base).unwrap(), input);
        assert_eq!(diffy::apply(&base, &Patch::from_str(&m.0).unwrap()).unwrap(), input);
    }

    /// changes apart - more than their context - commute; close ones do not
    #[test]
    fn far_changes_do_not_intersect_close_ones_do() {
        let base = lines(30);
        let a = mismatch(&base, &change(&base, 5, "a"));
        assert!(!a.is_intersect(&mismatch(&base, &change(&base, 20, "b"))).unwrap());
        // context 3 lines each side: line 8 is read by both
        assert!(a.is_intersect(&mismatch(&base, &change(&base, 11, "b"))).unwrap());
        assert!(a.is_intersect(&mismatch(&base, &change(&base, 5, "b"))).unwrap());
        // lines 2..8 and 9..15 touch, do not overlap
        assert!(!a.is_intersect(&mismatch(&base, &change(&base, 12, "b"))).unwrap());
    }

    /// the same change in both - a line changed or deleted alike - is no intersection,
    /// and applies once in either order
    #[test]
    fn the_same_change_is_no_intersection() {
        let base = lines(30);
        for input in [change(&base, 7, "seven"), base.replace("line 7\n", "")] {
            let a = mismatch(&base, &change(&input, 24, "a"));
            let b = mismatch(&base, &input);
            assert!(!a.is_intersect(&b).unwrap());
            let ab = b.apply(&a.apply(&base).unwrap()).unwrap();
            assert_eq!(ab, a.apply(&b.apply(&base).unwrap()).unwrap());
            assert_eq!(ab, change(&input, 24, "a"));
        }
    }

    /// lines added before the other's change move it, and it still applies
    #[test]
    fn a_shift_is_followed() {
        let base = lines(30);
        let a = mismatch(&base, &base.replace("line 2\n", "line 2\nnew\nnewer\n"));
        let b = mismatch(&base, &change(&base, 20, "b"));
        assert!(!a.is_intersect(&b).unwrap());
        let ab = b.apply(&a.apply(&base).unwrap()).unwrap();
        assert_eq!(ab, a.apply(&b.apply(&base).unwrap()).unwrap());
        assert!(ab.contains("newer\n") && ab.contains("\nb\n"), "{}", ab);
    }

    /// into an empty base: both insert at its start
    #[test]
    fn inserts_into_an_empty_base_intersect() {
        let a = mismatch("", "a\n");
        assert!(a.is_intersect(&mismatch("", "b\n")).unwrap());
        assert_eq!(a.apply(&String::new()).unwrap(), "a\n");
    }

    /// a hunk whose lines are not there, nor its result
    #[test]
    fn a_hunk_that_does_not_apply() {
        let base = lines(10);
        let m = mismatch(&base, &change(&base, 5, "five"));
        let e = m.apply(&lines(4)).unwrap_err();
        assert!(matches!(e, DocError::PatchHunk { hunk: 1 }), "{:?}", e);
        assert!(e.to_string().starts_with("DD-E202 "), "{}", e);
    }

    /// a patch that does not parse
    #[test]
    fn a_patch_that_does_not_parse() {
        let bad = Mismatch("@@ -x @@\n".to_string());
        assert!(matches!(bad.apply(&String::new()), Err(DocError::PatchParse(_))));
        assert!(bad.is_intersect(&bad).is_err());
        assert_eq!(bad.len(), 0);
    }
}
