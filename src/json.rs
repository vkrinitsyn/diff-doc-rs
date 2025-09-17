use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::iter::repeat;
use serde_json::Value;
use crate::{DocError, MismatchDoc, MismatchDocMut};

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mismatch(Vec<Hunk>);

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// chunk of changes
pub struct Hunk {
    /// json path
    #[serde(rename ="p")]
    path: Vec<DocIndex>,
    /// None for remove at path specified
    #[serde(rename ="v")]
    value: Option<Value>,
}

impl Display for Hunk {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self).unwrap_or_else(|e| format!("ERROR: {}", e)))
    }
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// value of changes
pub enum HunkVo {
    /// remove
    Remove,
    /// change value
    Value(Value),
    /// change array index with shift right
    Reindex(usize),
    /// obj name
    Rename(String),
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// chunk of changes
pub struct Hunk2 {
    /// json path
    #[serde(rename ="p")]
    pub(crate) path: Vec<DocIndex>,
    /// None for remove at path specified
    #[serde(rename ="v")]
    pub(crate) value: HunkVo,
}

impl Hunk {
    fn new_rm(path: &Vec<DocIndex>) -> Self {
        Hunk{path: path.clone(), value: None}
    }

    fn new_v(path: &Vec<DocIndex>, v: &Value) -> Self {
        Hunk{path: path.clone(), value: Some(v.to_owned())}
    }

    fn new_a(path: &Vec<DocIndex>, append: usize, value: Option<&Value>) -> Self {
        let mut path = path.to_owned();
        path.push(DocIndex::Idx(append));
        Hunk{path, value: value.map(|v| v.to_owned())}
    }

    fn is_none(&self) -> bool {
        match &self.value {
            None => true,
            Some(_) => false
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DocIndex {
    #[serde(rename ="n")]
    Name(String),
    #[serde(rename ="i")]
    Idx(usize)
}

impl DocIndex {
    #[cfg(test)]
    fn new(value: &String) -> Result<Vec<DocIndex>, DocError> {
        let path: Vec<&str> = value.split(".").collect();
        let mut r = Vec::with_capacity(path.len());
        for p in path {
            if p.len() == 0 {
                continue;
            }
            if p.starts_with("[") { // path element is an array index
                if p.len() > 2 {
                    let idx = p[1..p.len() - 1].parse::<usize>()
                        .map_err(|e| DocError::new(e.to_string()))?;
                    r.push(DocIndex::Idx(idx));
                }
            } else {
                r.push(DocIndex::Name(p.to_string()));
            }
        }
        Ok(r)
    }

    #[cfg(test)]
    fn is_array(input: &Vec<Self>) -> bool {
        input.iter().find(|d| if let DocIndex::Idx(_) = d {true} else {false})
            .is_some()
    }

    #[cfg(test)]
    fn min_array(input: &Vec<Self>) -> Option<usize> {
        input.iter().filter_map(|d|
            match d {
                DocIndex::Name(_) => None,
                DocIndex::Idx(i) => Some(*i)
            }
        )
            .min()
    }

}

impl MismatchDocMut<Value> for Mismatch {
    fn apply_mut(&self, input: &mut Value) -> Result<(), DocError> {
        for h in &self.0 {
            match &h.value {
                None => {
                    self.remove(&h.path, input);
                }
                Some(v) => {
                    self.modify(&h.path, v.clone(), input);
                }
            }
        }
        Ok(())
    }
}

impl  Mismatch {

    fn remove(&self, path: &Vec<DocIndex>, json_root: &mut Value) {
        let mut input = json_root;  // current json node pointer
        for path_index in 0..path.len() {
            let last_element = path_index == path.len() - 1;
            match &path[path_index] {
                DocIndex::Name(path) => {
                    if input.is_object() {
                        if last_element { // do delete
                            input.as_object_mut().unwrap().remove(path);
                        } else { // do traverse
                            match input.as_object_mut().unwrap().get_mut(path) {
                                None => {
                                    // no such field for go into for removal - ignore
                                    return;
                                }
                                Some(m) => {
                                    input = m;
                                }
                            }
                        }
                    } else {
                        // discrepancy on path: expected object, but it is not
                        return;
                    }
                }
                DocIndex::Idx(idx) => {
                    if input.is_array() {
                        if last_element { // do delete
                            input.as_array_mut().unwrap().remove(*idx);
                        } else { // do traverse
                            match input.as_array_mut().unwrap().get_mut(*idx) {
                                None => {
                                    // no such field for go into for removal - ignore
                                    return;
                                }
                                Some(m) => {
                                    input = m;
                                }
                            }
                        }
                    } else {
                        // discrepancy on path: expected array, but it is not
                        return;
                    }
                }
            }
        }
    }

    fn modify(&self, path: &Vec<DocIndex>, value: Value, json_root: &mut Value) {
        let mut input = json_root; // current json node pointer
        for path_index in 0..path.len() {
            let last_element = path_index == path.len() - 1;
            match &path[path_index] {
                DocIndex::Name(path) => {
                    if last_element {
                        if input.is_object() { //
                            input.as_object_mut().unwrap().insert(path.to_string(), value);
                            return;
                        }
                        //
                    } else { // traverse
                        if input.get(path).is_none() { // set object if it is not
                            input.as_object_mut().unwrap().insert(path.to_string(),
                                                                  Value::Object(serde_json::Map::new())
                            );
                        }
                        input = input.as_object_mut().unwrap().get_mut(path).unwrap();
                    }
                }
                DocIndex::Idx(idx) => {
                    if input.is_array() {
                        let input_len = input.as_array().map(|a| a.len()).unwrap_or(0);

                    for _i in input_len..*idx + 1 {
                        input.as_array_mut().unwrap().push(Value::Null);
                    }
                    if last_element {
                            // set value at the end of path
                            if let Some(e) = input.get_mut(idx) {
                                // set array element
                                *e = value;
                            }
                            return;
                        } else {
                            // set pointer to required array element to move to next path element
                            input = input.get_mut(idx).unwrap();
                        }
                    } else {
                        // mismatch types - expected array but found something else
                        // replace existing object with array
                        *input = Value::Array(repeat(Value::Null.clone()).take(*idx + 1).collect());
                        if last_element {
                            *input.as_array_mut().unwrap().get_mut(*idx).unwrap() = value;
                            return;
                        }
                    }
                }
            }
        }
    }

}

impl MismatchDoc<Value> for Mismatch {
    fn new(base: &Value, input: &Value) -> Result<Self, DocError>
    where
        Self: Sized
    {
        Ok(Mismatch(jdiff(base, &input, &vec![])))
    }

    fn is_intersect(&self, input: &Self) -> Result<bool, DocError> {
        for a in &self.0 {
            if input.0.iter().find_map(|b| Some( is_intersect(a, b) || is_intersect(b, a) ))
                .unwrap_or(false) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

/// check for intersection of two patches by path for update or delete of documents including vec/array
fn is_intersect(a: &Hunk, b: &Hunk) -> bool {
    if a.path.len() == 0 || b.path.len() == 0 {
        return false; // assert changes
    }

    // this is a json path index, the longer path wont intersect with short one if longer do not contain the short
    let comp2idx = min(a.path.len(), b.path.len());

    for i in 0..comp2idx {
        if let Some(cause) = is_intersect2(a,b,i, !(a.path.len()==b.path.len() && i==comp2idx)) {
            #[cfg(debug_assertions)] println!("is_intersect as step {i} of {comp2idx} by: {cause}\n{a}\n{b}");
        } else {
            return false;
        }
    }
    true
}

/// check for intersection of two patches by path for update or delete of documents including vec/array
fn is_intersect2(a: &Hunk, b: &Hunk, idx: usize, ignore_val: bool) -> Option<&'static str> {
    match &a.path[idx] {
        DocIndex::Name(a_path) => {
            match &b.path[idx] {
                DocIndex::Name(b_path) => {
                    if a_path == b_path && (ignore_val || &a.value != &b.value) {
                        Some("diff values ")
                    } else { None }
                }
                DocIndex::Idx(_) => {
                    if !ignore_val || a.value.is_none() || b.value.is_none() {
                        Some("discrepancy in types name-idx, but in case of delete - no matter")
                    } else { None }
                }
            }
        }
        DocIndex::Idx(a_idx) => {
            match &b.path[idx] {
                DocIndex::Name(_) => {
                    if !ignore_val || a.value.is_none() || b.value.is_none() {
                    Some("discrepancy in types idx-name, but in case of delete - no matter")
                    } else { None }
                }
                DocIndex::Idx(b_idx) => {
                    match &a.value {
                        None => {
                            match &b.value {
                                None => Some("remove both - undefined second remove index after first remove & shift"),
                                Some(_) => {
                                    if  !ignore_val && a_idx > b_idx {
                                        Some("update b, remove a")
                                    } else { None }
                                }
                            }
                        }
                        Some(a_value) => {
                            match &b.value {
                                None => {
                                    if !ignore_val && b_idx > a_idx {
                                        Some("update a, remove b")
                                    } else { None }
                                }
                                Some(b_value) => {
                                    if a_idx == b_idx && (!ignore_val || (ignore_val && a_value != b_value)) {
                                        Some("intersect only on same fields and unequal new values")
                                    } else { None }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// traverse to all json tree and clean the input intersect with base, so remining input will be added to discrepancy
/// return the tree is empty
fn jdiff(base: &Value, input: &Value, path: &Vec<DocIndex>) -> Vec<Hunk> {
    if base.is_null() && !input.is_null() {
        vec![Hunk::new_v(path, input)]
    } else if !base.is_null() && input.is_null() {
        vec![Hunk::new_rm(path)]
    } else if base.is_array() {
        let mut diff = Vec::new();
        if let Some(base_arr) = base.as_array() {
            if let Some(input_arr) = input.as_array() {
                if base_arr.len() == 0 {   // append all
                    diff.push(Hunk::new_v(path, input));
                }
                // todo rewrite to support insert - use identify_min_vec_changes
                if input_arr.len() > base_arr.len() {
                    for i in base_arr.len()..input_arr.len() {
                        diff.push(Hunk::new_a(path, i, input_arr.get(i)));
                    }
                }
                // if input_arr.len() < base_arr.len() {
                let mut d = identify_min_vec_changes(base_arr, input_arr, path);
                if d.len() > 0 {
                    diff.append(&mut d);
                } else {
                    // todo move for loop into identify_min_vec_changes
                    for i in 0..base_arr.len() {
                        let idx = base_arr.len() - i - 1;
                        match input_arr.get(idx) {
                            None => {
                                if base_arr.get(idx).is_some() {
                                    diff.push(Hunk::new_a(path, idx, None)); // remove array element
                                }
                            }
                            Some(v) => {
                                if v.is_object() && !v.is_null() {
                                    let mut path = path.to_owned();
                                    path.push(DocIndex::Idx(idx));
                                    diff.append(&mut jdiff(base_arr.get(idx).unwrap(), v, &path)); // compare and insert
                                } else if v != base_arr.get(idx).unwrap_or(&Value::Null) {
                                    diff.push(Hunk::new_a(path, idx, Some(v)));
                                }
                            }
                        }
                    }
                }
            } else { // unmatch types - the new value is not arrays
                diff.push(Hunk::new_v(path, input));
            }
        }
        diff
    } else if base.is_object() {
        let mut diff = Vec::new();
        if let Some(b) = base.as_object() {
            if let Some(input) = input.as_object() {
                for (key, val) in b {
                    let mut p = path.clone();
                    p.push(DocIndex::Name(key.clone()));
                    if let Some(input) = input.get(key) {
                        diff.append(&mut jdiff(val, input, &p));
                    } else {
                        diff.push(Hunk::new_rm(path));
                    }
                }

                if b.len() < input.len() {
                    for (key, input) in input.iter()
                        .filter(|(k, _)| !b.contains_key(*k)) {
                        append(input, &path, DocIndex::Name(key.clone()), &mut diff);
                    }
                }
            } else { // unmatch types - the new value is not object
                diff.push(Hunk::new_v(path, input));
            }
        }
        diff
    // object elements not null, objects nor array, present in this, may or may not be equal to input, so remove from input anyway
    } else if base != input {
        vec![Hunk{path: path.to_owned(), value: if input.is_null() { None } else { Some(input.to_owned()) }}]
    } else {
        vec![]
    }
}

/// Heuristic search to remove some index from base
/// return 0 or input_arr.len() if input_arr.len() < base_arr.len()

fn identify_min_vec_changes(base_arr: &Vec<Value>, input_arr: &Vec<Value>, path: &Vec<DocIndex>) -> Vec<Hunk> {
    let mut diff = Vec::new();

    let mut map = HashSet::with_capacity(input_arr.len());
    for v in input_arr {
        map.insert(hs(v));
    }
    #[cfg(debug_assertions)] println!("\n{:?}\n", map);
    let mut idx = 0;
    for v in base_arr {
        if !map.contains(&hs(v)) {
            diff.push(Hunk::new_a(path, idx, None));
        }
        idx +=1;
    }
    diff
}

/// Heuristic search to remove some index from base
/// return 0 or input_arr.len() if input_arr.len() < base_arr.len()
// todo:
//  1. array element move from index to index
//  2. array element delete
//  3. array object minimal changes
fn identify_min_vec_changes2(base_arr: &Vec<Value>, input_arr: &Vec<Value>, path: &Vec<DocIndex>) -> Vec<Hunk2> {

    let mut diff = Vec::new();
    let mut map = HashMap::with_capacity(input_arr.len());
    let mut idx = 0;
    for v in input_arr {
        map.insert(hs(v), idx);
        idx +=1;
    }
    #[cfg(debug_assertions)] println!("\n{:?}\n", map);
    for i in 0..base_arr.len() {
        let idx = base_arr.len() - i;
        if !map.contains_key(&hs(&base_arr[idx])) {
            let mut path = path.clone();
            path.push(DocIndex::Idx(idx));
            diff.push(Hunk2{ path, value: HunkVo::Remove });
        }
    }
    diff
    /*
        for i in 0..base_arr.len() {
            let idx = base_arr.len() - i - 1;
            match input_arr.get(idx) {
                None => {
                    if base_arr.get(idx).is_some() {
                        append_insert(map, path, idx, None); // remove array element
                    }
                }
                Some(v) => {
                    if v.is_object() && !v.is_null() {
                        let mut path = path.to_owned();
                        path.push(DocIndex::Idx(idx));
                        jdiff(base_arr.get(idx).unwrap(), v, &path, map); // compare and insert
                    } else if v != base_arr.get(idx).unwrap_or(&Value::Null) {
                        append_insert(map, path, idx, Some(v));
                    }
                }
            }
        }
   */
    // } else {
    //     0
    // }
}

fn hs(input: &Value) -> u64 {
    // let input = serde_json::to_string(input).expect("Failed to serialize JSON");
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let h = hasher.finish();
    #[cfg(debug_assertions)] println!("{input} = {h}");
    h
}

/// check remines on input object. extract all object if single field or element array
/// define the longest path to the unmatched object set or field or array
// TODO test case: [1,2,3] -> [,2,3] | [1,,3] as patch json: { "key.[0]": }
fn append(input: &Value, path: &Vec<DocIndex>, path_append: DocIndex, diff: &mut Vec<Hunk>) {
    let mut path= path.to_owned();
    path.push(path_append);
    match input {
        Value::Null => { } // do nothing
        Value::Array(a) => {
            if a.len() == 1 {
                if let Some(a) = a.get(0) {
                    append(a, &path, DocIndex::Idx(0), diff); // recursion with a new path
                }
            } else {
                diff.push(Hunk::new_v(&path, input));
            }
        }
        Value::Object(i) => {
            if i.len() == 1 {
                if let Some((k, v)) = i.iter().next() {
                    append(v, &path, DocIndex::Name(k.clone()), diff); // recursion with a new path
                }
            } else {
                diff.push(Hunk::new_v(&path, input));
            }
        }
        _ => {
            diff.push(Hunk::new_v(&path, input));
        }
    }
}


impl Display for Mismatch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self.0).unwrap_or_else(|e| format!("ERROR: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_idx() {
        assert_eq!(DocIndex::new(&"[]".to_string()).unwrap(), vec![]);
        assert_eq!(DocIndex::new(&".[].".to_string()).unwrap(), vec![]);
        assert_eq!(DocIndex::new(&".[0]".to_string()).unwrap(), vec![DocIndex::Idx(0)]);
        assert_eq!(DocIndex::new(&"[0]".to_string()).unwrap(), vec![DocIndex::Idx(0)]);
    }

    #[test]
    fn test_name() {
        assert_eq!(DocIndex::new(&".".to_string()).unwrap(), vec![]);
        assert_eq!(DocIndex::new(&"a".to_string()).unwrap(), vec![DocIndex::Name("a".to_string())]);
        assert_eq!(DocIndex::new(&".a".to_string()).unwrap(), vec![DocIndex::Name("a".to_string())]);
        assert_eq!(DocIndex::new(&".a.".to_string()).unwrap(), vec![DocIndex::Name("a".to_string())]);
    }

    #[test]
    fn test_name_idx() {
        assert_eq!(DocIndex::new(&".a.[1].b".to_string()).unwrap(),
                   vec![DocIndex::Name("a".to_string()), DocIndex::Idx(1), DocIndex::Name("b".to_string())]);
    }


    #[test]
    fn test_is_intersect() {
        assert!(!is_intersect(&Hunk{path: vec![], value: None},
                              &Hunk{path: vec![], value: None}));
        assert!(!is_intersect(&Hunk{path: vec![DocIndex::Idx(0)], value: None},
                              &Hunk{path: vec![], value: None}));
        assert!(!is_intersect(&Hunk{path: vec![], value: None},
                              &Hunk{path: vec![DocIndex::Idx(0)], value: None}));
   }

    #[test]
    fn test_h_1() {
        assert_eq!(Mismatch::new(&json!(["a", "b", "c"]), &json!(["b", "c"])).unwrap(),
                   Mismatch(vec![Hunk{path: vec![DocIndex::Idx(0)], value: None}] )
        );

        assert_eq!(Mismatch::new(&json!(["a", "b", "c"]), &json!(["a", "c"])).unwrap(),
                   Mismatch(vec![Hunk{path: vec![DocIndex::Idx(1)], value: None}] )
        );

        assert_eq!(Mismatch::new(&json!(["a", "b", "c"]), &json!(["a", "b"])).unwrap(),
                   Mismatch(vec![Hunk{path: vec![DocIndex::Idx(2)], value: None}] )
        );
    }

    #[test]
    fn test_ia_del0() {
        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(0)], value: None},
                              &Hunk{path: vec![DocIndex::Idx(1)], value: None}));
        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(1)], value: None},
                              &Hunk{path: vec![DocIndex::Idx(0)], value: None}));
        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(0)], value: None},
                              &Hunk{path: vec![DocIndex::Idx(1)], value: None}));
    }

    #[test]
    fn test_ia_del() {
        assert!(is_intersect(&Hunk{path: vec![DocIndex::Name("a".into())], value: None},
                             &Hunk{path: vec![DocIndex::Idx(0)], value: None}));

        assert!(is_intersect(&Hunk{path: vec![DocIndex::Name("a".into())], value: Some(json!({}))},
                             &Hunk{path: vec![DocIndex::Idx(0)], value: None}));

        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(0)], value: None},
                             &Hunk{path: vec![DocIndex::Name("a".into())], value: None}));

        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(0)], value: Some(json!({}))},
                             &Hunk{path: vec![DocIndex::Name("a".into())], value: None}));

    }

    #[test]
    fn test_a_0() {
        assert_eq!(Mismatch::new(&json!(["a","b"]), &json!(["b","b"])).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Idx(0)], value: Some(json!("b"))}] )
        )
    }

    #[test]
    fn test_a_1() {
        assert_eq!(Mismatch::new(&json!(["a","b"]), &json!(["a","c"])).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Idx(1)], value: Some(json!("c"))}] )
        )
    }

    #[test]
    fn test_a_2() {
        assert_eq!(Mismatch::new(&json!(["a","b"]), &json!(["a"])).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Idx(1)], value: None}] )
        )
    }

    #[test]
    fn test_a_4() {
        assert_eq!(Mismatch::new(&json!(["a","b"]), &json!(["c", "b"])).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Idx(0)], value: Some(json!("c"))}] )
        )
    }

    #[test]
    fn test_a_5() {
        assert_eq!(Mismatch::new(&json!(["a","b"]), &json!(["a", "b", "c"])).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Idx(2)], value: Some(json!("c"))}] )
        )
    }

    #[test]
    fn test_a_6() {
        assert_eq!(Mismatch::new(&json!([{"a":"b"},{"a":"d"}]), &json!([{"a":"b"}])).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Idx(1)], value: None}] )
        )
    }

    #[test]
    fn test_a_7() {
        assert_eq!(Mismatch::new(&json!({"arr":[{"a":"b"},{"a":"d"}]}),
                                 &json!({"arr":[{"a":"c"}]})).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Name("arr".into()), DocIndex::Idx(1)], value: None},
                          Hunk { path: vec![DocIndex::Name("arr".into()), DocIndex::Idx(0), DocIndex::Name("a".into())], value: Some(json!("c")) }] )
        )
    }

    #[test]
    fn test_oa_1() {
        assert_eq!(Mismatch::new( &json!([{"a":"a"}, {"b":"b"}]),
                                 &json!([{"a":"a"}, {"b":"c"}])).unwrap(),
            Mismatch(vec![Hunk{path: vec![DocIndex::Idx(1), DocIndex::Name("b".into())],
                value: Some(json!("c"))}] )
        )
    }

    #[test]
    fn test_oa_2() {
        assert_eq!(Mismatch::new( &json!({"a":["b","c"], "d":["e","f"]}),
                                 &json!({"a":["b","c"], "d":["e","g"]})).unwrap(),
                   Mismatch(vec![Hunk{path: vec![DocIndex::Name("d".into()), DocIndex::Idx(1)],
                       value: Some(json!("g"))}] )
        )
    }

    #[test]
    fn test_o_1() {
        assert_eq!(Mismatch::new(&json!({"a":"b", "c":"d"}),
                                 &json!({"a":"b"})).unwrap(),
                   Mismatch(vec![Hunk {path: vec![DocIndex::Name("c".into())], value: None }] )
        )
    }

    #[test]
    fn test_o_2() {
        assert_eq!(Mismatch::new(&json!({"a":"b", "c":"d"}),
                                 &json!({"a":"b", "c":"d", "e":"f"})).unwrap(),
                   Mismatch(vec![Hunk {path: vec![ DocIndex::Name("e".into())], value: Some(json!("f")) }] )
        )
    }

    #[test]
    fn test_o_3() {
        assert_eq!(Mismatch::new(&json!({"a":"b", "c":"d", "e":"f"}),
                                 &json!({"a":"b", "e":"f"})).unwrap(),
                   Mismatch(vec![Hunk {path: vec![ DocIndex::Name("c".into())], value: None }] )
        )
    }

    #[test]
    fn test_o_4() {
        assert_eq!(Mismatch::new(&json!({"a":"b", "c":"d"}),
                                 &json!({"a":"x", "c":"d"})).unwrap(),
                   Mismatch(vec![Hunk {path: vec![DocIndex::Name("a".into())], value: Some(json!("x")) }] )
        )
    }

    #[test]
    fn test_o_5() {
        assert_eq!(Mismatch::new(&json!({"a":"b", "c":"d"}),
                                 &json!({"a":"b", "c":"x"})).unwrap(),
                   Mismatch(vec![Hunk {path: vec![DocIndex::Name("c".into())], value: Some(json!("x")) }] )
        )
    }

    #[test]
    fn test_o_6() {
        assert_eq!(Mismatch::new(&json!({"a":"b", "c":"d"}),
                                 &json!({"a":"b", "c":"d"})).unwrap(),
                   Mismatch(vec![] )
        )
    }

}