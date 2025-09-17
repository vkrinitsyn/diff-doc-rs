use std::cmp::max;
use crate::generic::{hs, DocIndex, GenericValue, Hunk, HunkAction};
use std::collections::HashMap;

/// identify minimum changes of array elements from old to new with remove, swap, clone, update and insert operations
/// operations as Hunk list are based on context path and use append_path to create full path
pub fn compute_vec_diff(old: &Vec<GenericValue>, new: &Vec<GenericValue>, context_path: &Vec<DocIndex>) -> Vec<Hunk> {
    let mut updates: Vec<Hunk> = Vec::new(); // resulting operations
    let mut workspace: Vec<(usize, u64)> = Vec::with_capacity(old.len());//(0..old.len() - 1).collect(); // indices of old that will be processed
    let mut sources: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, v) in old.iter().enumerate() {  // hash to (index, is_used and must not remove operation)
        let hash = hs(v);
        sources.entry(hash).or_insert_with(Vec::new).push(i);
        workspace.push((i, hash));
    }
    let mut targets: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, v) in new.iter().enumerate() {  // hash to (index, is_used and must not remove operation)
        targets.entry(hs(v)).or_insert_with(Vec::new).push(i);
    }

    // implement strategy based of function docs:
    //  - identify shift indices left or right in old as we go to get a target index in new
    //  - identify new indexes as cloned to minimize data transfer
    //  - identify updates where hash matches but value differs
    //  - identify where updates less changes after clone vs insert
    //  - identify indexes that are swapped
    //  - identify indexes that are removed
    //  - identify indexes that are inserted with complete new values
    //  - use workspace to track used indices in old

    // 1.  Iterate over new, if old by Id by workspace not equal define:
    // Shift or not shift i.e. Update or insert, search it old,
    //  if found index less, than clone, if more than swap,
    // Do search 1/10 min 2 elements ahead to identify for removal with shift left
    //  if not found, than use clone with possible updates:
    //      if more effective or insert
    for (new_index, new_value) in new.iter().enumerate() {
        if new_index >= workspace.len() {
            // no future insert to workspace for all remining new values
            updates.push(Hunk {
                path: append_path(context_path, new_index),
                value: HunkAction::Insert(new_value.clone()),
            });
        } else {
            let new_hash = hs(new_value);
            let (work_index, work_hash) = workspace[new_index];
            if work_hash != new_hash {
                let path = append_path(context_path, work_index);
                match sources.get_mut(&new_hash) {
                    Some(founded_indices) => {
                        let mut ii = 0;
                        for index in 0..founded_indices.len() {
                            if founded_indices[index] > work_index {
                                ii = index;
                                break;
                            }
                        }

                        let fi = founded_indices[ii];

                        if fi < work_index {
                            // clone
                            updates.push(Hunk { path, value: HunkAction::Clone(DocIndex::Idx(fi)) });
                            workspace.insert(work_index, (fi, new_hash));
                        } else {
                            let not_used = workspace[work_index+1..fi].iter()
                                .find(|(_, h)| targets.contains_key(h)).is_none();
                            // if diff less than 2%, than remove else swap
                            if not_used && fi - work_index > max(2, new.len()/50) {
                                // remove all between old_index and fi
                                for _ in work_index..fi {
                                    updates.push(Hunk {path: path.clone(), value: HunkAction::Remove});
                                    workspace.remove(work_index);
                                }
                                founded_indices[ii] = work_index;
                            } else {
                                updates.push(Hunk { path, value: HunkAction::Swap(DocIndex::Idx(fi)) });
                                workspace.swap(work_index, fi);
                            }
                        }
                    }
                    None => {
                        if work_index < old.len() {
                            compare_apply(&old[work_index], new_value, new_hash, path, &mut updates, &mut workspace, work_index);

                        } else {
                            updates.push(Hunk { path, value: HunkAction::Insert(new_value.clone()) });
                        }

                    }
                }
            } // on else - no actions needed, same value

        }
    }
    updates
}

// todo try update existing object and compare with insert by json size produced
fn compare_apply(_to_update: &GenericValue, to_insert: &GenericValue, hash: u64, path: Vec<DocIndex>, updates: &mut Vec<Hunk>, workspace: &mut Vec<(usize, u64)>, work_index: usize) {
    updates.push(Hunk { path, value: HunkAction::Update(to_insert.clone()) });
    workspace[work_index] = (work_index, hash);
}


fn append_path(path: &Vec<DocIndex>, index: usize) -> Vec<DocIndex> {
    let mut p = path.clone();
    p.push(DocIndex::Idx(index));
    p
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Range {
    pub(crate) start: usize,
    /// will set to Some when next different op type found
    pub(crate) end: Option<usize>,
    /// the range starts on insert (true) or delete (false)
    pub(crate) add: bool,
}

impl Range {

    pub(crate) fn overlap(&self, other: &Self) -> bool {
        let s1 = self.start;
        let e1 = self.end.unwrap_or(usize::MAX);
        let s2 = other.start;
        let e2 = other.end.unwrap_or(usize::MAX);
        !(e1 < s2 || e2 < s1)
    }
/*
    fn new(input: &Vec<DiffVecOp>) -> Vec<Self> {
        let mut ranges: Vec<Range> = Vec::new();
        for op in input {
            let add = match op {
                DiffVecOp::Remove { .. } => {
                    false
                },
                DiffVecOp::Insert { .. } => {
                    true
                }
                _ => {
                    continue;
                }
            };
            if ranges.len() == 0 || ranges[ranges.len()-1].add != add {
                ranges.push(Range{ start: op.index(), end: None, add });
            } else {
                let mut found = false;
                for r in ranges.iter_mut().rev() {
                    if r.add == add && r.end.is_none() {
                        r.end = Some(op.index());
                        found = true;
                        break;
                    }
                }
                if !found {
                    ranges.push(Range{ start: op.index(), end: None, add });
                }
            }
        }
        ranges
    }

*/
}

/*
pub struct MismatchVec(Vec<Hunk>);

impl MismatchDoc<Vec<GenericValue>> for MismatchVec {
    fn new(base: &Vec<GenericValue>, input: &Vec<GenericValue>) -> Result<Self, DocError>
    where
        Self: Sized {
        Ok(MismatchVec(compute_vec_diff(base, input, &vec![])))
    }

    fn is_intersect(&self, other: &Self) -> Result<bool, DocError> {
        let ranges = Range::new(&other.0);
        for r1 in &Range::new(&self.0) {
            for r2 in &ranges {
                if r1.overlap(r2) {
                    return Ok(true);
                }
            }
        }

        // there is no lines to delete (None value) and update after
        for k in &self.0 {
            if other.0.iter().find_map(|v| Some(v.unmatch(k))).unwrap_or(false) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}


impl Mismatch {
    fn min2delete(&self) -> Option<usize> {
        self.0.iter()
            .filter(| v| v.is_delete_insert())
            .map(|k| k.index()).min()
    }

    fn max2update(&self) -> Option<usize> {
        self.0.iter()
            .filter(| v| !v.is_delete_insert())
            .map(|k| k.index()).max()
    }

    /// the minimum remove index (None in value) must be greater than update
    fn _valid(&self) -> bool {
        self.min2delete()
            .map(|m| m >= self.max2update().unwrap_or(0))
            .unwrap_or(true)
    }
}
*/

#[cfg(test)]
mod tests {
    #[test]
    fn parsing_test_1() {

    }

}
