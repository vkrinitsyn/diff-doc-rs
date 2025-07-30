use std::cmp::{max, min};
use std::collections::HashSet;
use diffy::{create_patch, HunkRange, Patch};
use crate::{DocError, MismatchDoc};

/// wrapper to diffy patches with intersect calculation
///  - Text file format as https://en.wikipedia.org/wiki/Diff
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mismatch (String);

impl MismatchDoc<String> for Mismatch {
    fn new(base: &String, input: &String) -> Result<Self, DocError>
    where
        Self: Sized
    {
        Ok(Mismatch(create_patch(base, input).to_string()))
    }

    fn apply_to(&self, base: &String) -> Result<String, DocError> {
        diffy::apply(base, &Patch::from_str(base.as_str())
            .map_err(|e| DocError::new(e.to_string()))?)
            .map_err(|e| DocError::new(e.to_string()))
    }

    fn is_intersect(&self, input: &Self) -> Result<bool, DocError> {
        Ok(is_intersect_patch(
            &Patch::from_str(self.0.as_str())
            .map_err(|e| DocError::new(e.to_string()))?,
            &Patch::from_str(input.0.as_str())
            .map_err(|e| DocError::new(e.to_string()))?))
    }
}


/// Calculate real range size:
/// The HunkRange present 3 lines before and 3 lines after a changed lines,
/// those six lines use as marker and do not contain changes
fn irange(h: &HunkRange, diff: &mut HashSet<usize>) -> usize {
    let range = max(h.len(), 7);
    for i in h.start()+range / 2 .. h.end()+1-range / 2 {
        diff.insert(i);
    }
    h.len()
}

/// Text intersect calculation of two Patches:
/// - not intersect if even and not same lines
/// - is intersected if adding / removing lines (marked uneven) less than other
///
/// Returns: 
/// - minimum number of line when ok as adding / err as removing  lines
/// - set of changing lines
fn ranges(patch: &Patch<str>) -> (Result<usize, usize>, HashSet<usize>) {
    let mut diff = HashSet::new();
    let mut u = false;
    let mut min_u = < usize > :: MAX - 1usize;
    let mut max_u = 0;
    
    for h in patch.hunks() {
        let old = irange(&h.old_range(), &mut diff);
        let new = irange(&h.new_range(), &mut diff);
        if old !=new {
            u = true;
            min_u = min(min_u, old);
            min_u = min(min_u, new);
        } else {
            max_u = max(max_u, old);
            max_u = max(max_u, new);
        }
    }

    (if u { Err(min_u) } else {Ok(max_u)}, diff)
}


/// check for intersections i.e. unable to implement commutative for two patches
/// use diffy::apply(base_image, &patch) to modify
/// todo ignore same changes on same line include same line deletion
fn is_intersect_patch(patch_a: &Patch<str>, patch_b: &Patch<str>) -> bool {
    let (even_a, diff_a) = ranges(patch_a);
    let (even_b, diff_b) = ranges(patch_b);
    let intersect = match even_a {
        Ok(a) => match even_b {
            Ok(b) => a == b, // neither patches adding/removing, needs check modified lines
            Err(b) => a >= b, // only B patch adding/removing  
        },
        Err(a) => match even_b {
            Ok(b) => a <= b, // only A patch adding/removing  
            Err(_) => true, // both patches adding/removing, the intersect is essential
        }
    };
    // if not intersect yet, check modified lines joints
    intersect || !diff_a.is_disjoint(&diff_b)
}
