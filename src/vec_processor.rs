use std::cell::RefCell;
use crate::generic::{hs, DocIndex, GenericValue, Hunk, HunkAction};
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use crate::map_processor::min_map_changes;

#[derive(Clone, Debug)]
struct Idx {
    original_idx: usize,
    hash: u64,
}
impl Idx {
    fn new(idx: usize, hash: u64) -> Self {
        Self { original_idx: idx, hash }
    }
}

fn find_src(src: &HashMap<u64, Vec<Weak<RefCell<Idx>>>>,  sources: &Vec<Rc<RefCell<Idx>>>, hash: u64, work_idx: usize) -> Option<usize> {
    if src.contains_key(&hash) {
        for i in work_idx..sources.len() {
            if hash == sources[i].borrow().hash {
                return Some(i);
            }
        }
    }
    None
}

fn find_target(src: &HashMap<u64, Vec<usize>>, hashs: &Vec<Rc<RefCell<Idx>>>, work_idx: usize, found_idx: usize) -> bool {
    for rfi in work_idx..found_idx {
        let r = &hashs[rfi];
        if let Some(v) = src.get(&r.borrow().deref().hash) {
            for i in v {
                if *i > work_idx {
                    return true;
                }
            }
        }
    }
    false
}


/// identify minimum changes of array elements from old to new with remove, swap, clone, update and insert operations
/// operations as Hunk list are based on context path and use append_path to create full path
pub fn compute_vec_diff(old: &Vec<GenericValue>, new: &Vec<GenericValue>, context_path: &Vec<DocIndex>) -> Vec<Hunk> {
    let mut updates: Vec<Hunk> = Vec::new(); // resulting operations
    // workspace of indices in old, that are not yet used
    let mut workspace: Vec<Rc<RefCell<Idx>>> = Vec::with_capacity(old.len());
    // hash to list of weak references to indices in workspace
    let mut sources: HashMap<u64, Vec<Weak<RefCell<Idx>>>> = HashMap::new();
    for (i, v) in old.iter().enumerate() {
        let hash = hs(v);
        let r = Rc::new(RefCell::new(Idx::new(i, hash)));
        sources.entry(hash).or_insert_with(Vec::new).push(Rc::downgrade(&r));
        workspace.push(r);
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

    for (work_index, new_value) in new.iter().enumerate() {
        if work_index >= workspace.len() {
            // no future insert to workspace for all remining new values
            updates.push(Hunk {
                path: append_path(context_path, work_index),
                value: HunkAction::Insert(new_value.clone()),
            });
        } else {
            let new_hash = hs(new_value);
            let work_item = workspace[work_index].borrow().deref().clone();
            if work_item.hash != new_hash {
                let path = append_path(context_path, work_index);
                match find_src(&sources, &workspace, new_hash, work_index) {
                    Some(fi) => {
                        if fi < work_index {
                            updates.push(Hunk { path, value: HunkAction::Clone(DocIndex::Idx(fi)) });
                            let r = Rc::new(RefCell::new(Idx::new(work_index, new_hash)));
                            sources.entry(new_hash).or_insert_with(Vec::new).push(Rc::downgrade(&r));
                            workspace.insert(work_index, r);
                        } else {
                            if  find_target(&targets, &workspace, work_index, fi) {
                                updates.push(Hunk { path, value: HunkAction::Swap(DocIndex::Idx(fi)) });
                                workspace.swap(work_index, fi);
                            } else {
                                for _ in work_index..fi {
                                    updates.push(Hunk { path: path.clone(), value: HunkAction::Remove });
                                    workspace.remove(work_index);
                                }
                            }
                        }
                    }
                    None => {
                        if work_index < old.len() {
                            debug_assert!(workspace.len() > work_index);
                            compare_apply(&old[work_item.original_idx], new_value, new_hash, path,
                                          &mut updates, &mut workspace, &mut sources, work_index);
                        } else {
                            updates.push(Hunk { path, value: HunkAction::Insert(new_value.clone()) });
                        }

                    }
                }
            } // on else - no actions needed, same value

        }
    }
    if workspace.len() > new.len() {
        (0..workspace.len() - new.len()).for_each(|_| { // no need to modify sources as we are done
            updates.push(Hunk { path: append_path(context_path, new.len()), value: HunkAction::Remove });
        });
    }
    updates
}

/// update existing object
fn compare_apply(original: &GenericValue,
                 new_value: &GenericValue,
                 new_hash: u64,
                 path: Vec<DocIndex>,
                 updates: &mut Vec<Hunk>,
                 workspace: &mut Vec<Rc<RefCell<Idx>>>,
                 sources: &mut HashMap<u64, Vec<Weak<RefCell<Idx>>>>,
                 work_index: usize)
{

     if match original {
        GenericValue::Map(base_map) => {
            match new_value {
                GenericValue::Map(new_map) => {
                    let mut changes = min_map_changes(base_map, new_map, &path);
                    updates.append(&mut changes);
                    false
                }
                _ => true
            }
        }
        GenericValue::StringValue(base_txt) => {
            match new_value {
                GenericValue::StringValue(new_txt) => {
                    let ops = crate::txt::diff(base_txt, new_txt);
                    updates.push(Hunk { path: path.clone(), value: HunkAction::UpdateTxt(ops) });
                    false
                }
                _ => true
            }
        }
        _ => true
    } {
         updates.push(Hunk { path, value: HunkAction::Update(new_value.clone()) });
    }

    let r = Rc::new(RefCell::new(Idx::new(work_index, new_hash)));
    sources.entry(new_hash).or_insert_with(Vec::new).push(Rc::downgrade(&r));
    workspace[work_index] = r;
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
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::{Rc, Weak};
    use serde_json::json;
    use crate::diff::Mismatch;
    use crate::vec_processor::{compute_vec_diff, Idx};
    use crate::generic::{GenericValue, Hunk, HunkAction, DocIndex, hs, from_str_vec};
    use crate::{MismatchDoc, MismatchDocMut};

    #[test]
    fn test_compute_vec_diff_0() {
        if let GenericValue::Array(old) = serde_json::from_value(
            json!([1, "two", true, null, "five"])).unwrap() {
            if let GenericValue::Array(new) = serde_json::from_value(
                json!([1, "two", true, null, "five"])).unwrap() {
                let context_path = vec![];
                let diffs = compute_vec_diff(&old, &new, &context_path);
                assert_eq!(diffs.len(), 0);
            }
        }
    }

    #[test]
    fn test_weak_ref() {
        let old = vec![
            GenericValue::StringValue("1".to_string()),
            GenericValue::StringValue("1".to_string()),
            GenericValue::StringValue("3".to_string()),
            GenericValue::StringValue("4".to_string()),
            GenericValue::StringValue("five".to_string()),
        ];
        let mut workspace: Vec<Rc<RefCell<Idx>>> = Vec::with_capacity(old.len());
        let mut sources: HashMap<u64, Vec<Weak<RefCell<Idx>>>> = HashMap::new();
        for (i, v) in old.iter().enumerate() {
            let hash = hs(v);
            let r = Rc::new(RefCell::new(Idx::new(i, hash)));
            sources.entry(hash).or_insert_with(Vec::new).push(Rc::downgrade(&r));
            workspace.push(r);
        }

        println!("{:?} \n", workspace);
        let h = workspace[2].borrow().hash;
        let idx = sources.get(&h)
            .unwrap()[0].upgrade().unwrap().borrow().original_idx;
        println!("before {} ", idx);

        workspace[2].borrow_mut().original_idx = 10;
        let idx = sources.get(&workspace[2].borrow().hash)
            .unwrap()[0].upgrade().unwrap().borrow().original_idx;
        println!("after {} ", idx);
        // let f = find_src(&sources, h, 1);
        // assert_eq!(f, Some(10));
    }

    #[test]
    fn test_compute_vec_diff_1() {
        if let GenericValue::Array(old) = serde_json::from_value(
            json!(["1", "two", "3", "4", "five"])).unwrap() {
            if let GenericValue::Array(new) = serde_json::from_value(
                json!(["1", "3", "4"])).unwrap() {
                let context_path = vec![];
                let diffs = compute_vec_diff(&old, &new, &context_path);
                println!("{:?}", diffs);
                assert_eq!(diffs.len(), 2);
                assert_eq!(diffs[0], Hunk{path:vec![DocIndex::Idx(1)], value: HunkAction::Remove});
                assert_eq!(diffs[1], Hunk{path:vec![DocIndex::Idx(3)], value: HunkAction::Remove});
            }
        }
    }

    #[test]
    fn test_compute_vec_diff_2() {
        if let GenericValue::Array(old) = serde_json::from_value(
            json!(["1", "two", "two2", "3", "4", "five5", "five"])).unwrap() {
            if let GenericValue::Array(new) = serde_json::from_value(
                json!(["1", "3", "4"])).unwrap() {
                let context_path = vec![];
                let diffs = compute_vec_diff(&old, &new, &context_path);
                println!("{:?}", diffs);
                assert_eq!(diffs.len(), 4);
                assert_eq!(diffs[0], Hunk{path:vec![DocIndex::Idx(1)], value: HunkAction::Remove});
                assert_eq!(diffs[1], Hunk{path:vec![DocIndex::Idx(1)], value: HunkAction::Remove});
                assert_eq!(diffs[2], Hunk{path:vec![DocIndex::Idx(3)], value: HunkAction::Remove});
                assert_eq!(diffs[3], Hunk{path:vec![DocIndex::Idx(3)], value: HunkAction::Remove});
            }
        }
    }

   #[test]
    fn test_compute_vec_diff_swap1() {
        if let GenericValue::Array(old) = serde_json::from_value(
            json!(["1", "two", "3", "4", "five"])).unwrap() {
            if let GenericValue::Array(new) = serde_json::from_value(
                json!(["1", "3", "two", "4"])).unwrap() {
                let context_path = vec![];
                let diffs = compute_vec_diff(&old, &new, &context_path);
                println!("{:?}", diffs);
                assert_eq!(diffs.len(), 2);
                assert_eq!(diffs[0], Hunk{path:vec![DocIndex::Idx(1)], value: HunkAction::Swap(DocIndex::Idx(2))});
                assert_eq!(diffs[1], Hunk{path:vec![DocIndex::Idx(4)], value: HunkAction::Remove});
            }
        }
    }

   #[test]
    fn test_compute_vec_diff_swap2() {
        if let GenericValue::Array(old) = serde_json::from_value(
            json!(["1", "two", "3", "4", "five"])).unwrap() {
            if let GenericValue::Array(new) = serde_json::from_value(
                json!(["1", "3", "4", "two"])).unwrap() {
                let context_path = vec![];
                let diffs = compute_vec_diff(&old, &new, &context_path);
                println!("{:?}", diffs);
                assert_eq!(diffs.len(), 3);
                assert_eq!(diffs[0], Hunk{path:vec![DocIndex::Idx(1)], value: HunkAction::Swap(DocIndex::Idx(2))});
                assert_eq!(diffs[1], Hunk{path:vec![DocIndex::Idx(2)], value: HunkAction::Swap(DocIndex::Idx(3))});
                assert_eq!(diffs[2], Hunk{path:vec![DocIndex::Idx(4)], value: HunkAction::Remove});
            }
        }
    }

   #[test]
    fn test_compute_vec_diff_swap3() {
        if let GenericValue::Array(old) = serde_json::from_value(
            json!(["1", "two", "3", "4", "five"])).unwrap() {
            if let GenericValue::Array(new) = serde_json::from_value(
                json!(["1", "3", "five", "two"])).unwrap() {
                let context_path = vec![];
                let diffs = compute_vec_diff(&old, &new, &context_path);
                println!("{:?}", diffs);
                assert_eq!(diffs.len(), 3);

                assert_eq!(diffs[0], Hunk{path:vec![DocIndex::Idx(1)], value: HunkAction::Swap(DocIndex::Idx(2))});
                assert_eq!(diffs[1], Hunk{path:vec![DocIndex::Idx(2)], value: HunkAction::Swap(DocIndex::Idx(4))});
                assert_eq!(diffs[2], Hunk{path:vec![DocIndex::Idx(3)], value: HunkAction::Remove});
            }
        }
    }

    #[test]
    fn test_compute_vec_diff_appl() {
        let mut old = from_str_vec(vec!["a","b","c"]);
        let new = from_str_vec(vec!["a","b","d"]);
        let patch = Mismatch::new(&old, &new).unwrap();
        let result = patch.apply_mut(&mut old, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
        assert_eq!(old, new);
    }



}
