use std::cmp::min;
use std::fmt::{Display, Formatter};
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

impl Hunk {
    fn insert(v: &mut Vec<Self>, path: &Vec<DocIndex>, value: Option<&Value>) {
        v.push(Hunk{path: path.to_owned(), value: value.map(|v| v.to_owned())})
    }

    fn append_insert(v: &mut Vec<Self>, path: &Vec<DocIndex>, append: usize, value: Option<&Value>) {
        let mut path = path.to_owned();
        path.push(DocIndex::Idx(append));
        v.push(Hunk{path, value: value.map(|v| v.to_owned())})
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

    fn is_array(input: &Vec<Self>) -> bool {
        input.iter().find(|d| if let DocIndex::Idx(_) = d {true} else {false})
            .is_some()
    }

    fn min_array(input: &Vec<Self>) -> Option<usize> {
        input.iter().filter_map(|d|
            match d {
                DocIndex::Name(_) => None,
                DocIndex::Idx(i) => Some(*i)
            }
        )
            .min()
    }

    // fn insert(&mut self, path: Vec<DocIndex>, value: Option<Value>,) {
    //
    // }

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
                    // build array // if last_element || input_len - 1 < *idx {
                    for i in input_len..*idx + 1 {
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
        let mut diff = Vec::new();
        jdiff(base, &input, &mut vec![], &mut diff);
        Ok(Mismatch(diff))
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
    let key_a = &a.path;
    let val_a = &a.value;
    let key_b = &b.path;
    let val_b = &b.value;

    if key_a.len() == 0 || key_b.len() == 0{
        return false; // assert changes
    }
    let comp2idx = min(key_a.len(), key_b.len())-1;
    match &key_a[key_a.len()-1] {
        DocIndex::Name(a) => {
            match &key_b[comp2idx] {
                DocIndex::Name(b) => {
                    a==b && val_a!=val_b
                }
                DocIndex::Idx(_) => {
                    val_a.is_none() || val_b.is_none() // discrepancy in types, but in case of delete - no matter
                }
            }
        }
        DocIndex::Idx(a) => {
            match &key_b[comp2idx] {
                DocIndex::Name(_) => {
                    val_a.is_none() || val_b.is_none() // discrepancy in types, but in case of delete - no matter
                }
                DocIndex::Idx(b) => {
                    // todo get name on up & compare?
                    match val_a {
                        None => {
                            match val_b {
                                None => true, // remove both - undefined second remove index after first remove & shift
                                Some(_) => a>b // update b, remove a
                            }
                        }
                        Some(val_a) => {
                            match val_b {
                                None => b>a, // update a, remove b
                                Some(val_b) => a==b && val_a!=val_b // intersect only on same fields and unequal new values
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
fn jdiff(base: &Value, input: &Value, path: &mut Vec<DocIndex>, map: &mut Vec<Hunk>) {
    if base.is_null() && !input.is_null() {
        Hunk::insert(map, path, Some(input));
    } else if !base.is_null() && input.is_null() {
        Hunk::insert(map, path, None);
    } else if base.is_array() {
        if let Some(b) = base.as_array() {
            if let Some(a) = input.as_array() {
                for i in 0..b.len() {
                    let idx = b.len() - i -1;
                    match a.get(idx) {
                        None => {
                            if b.get(idx).is_some() {
                                Hunk::append_insert(map, path, idx, None); // remove array element
                            }
                        }
                        Some(v) => {
                            if v.is_object() && !v.is_null() {
                                path.push(DocIndex::Idx(idx));
                                jdiff(b.get(idx).unwrap(), v, path, map); // compare and insert
                            } else if v != b.get(idx).unwrap_or(&Value::Null) {
                                Hunk::append_insert(map, path, idx, Some(v));
                            }
                        }
                    }
                }
                if b.len() == 0 {   // append all
                    Hunk::insert(map, path, Some(input));
                } else if a.len() > b.len() {
                    for i in b.len()-1..a.len() {
                        Hunk::append_insert(map, path, i, a.get(i));
                    }
                }
            } else { // unmatch types - the new value is not arrays
                Hunk::insert(map, path, Some(input));
            }
        }
    } else if base.is_object() {
        if let Some(b) = base.as_object() {
            if let Some(input) = input.as_object() {
                for (key, val) in b {
                    let mut p = path.clone();
                    p.push(DocIndex::Name(key.clone()));
                    if let Some(input) = input.get(key) {
                        jdiff(val, input, &mut p, map);
                    } else {
                        Hunk::insert(map, &p, None);
                    }
                }

                if b.len() < input.len() {
                    for (key, input) in input.iter()
                        .filter(|(k, _)| !b.contains_key(*k)) {
                        append(input, path, DocIndex::Name(key.clone()), map);
                    }
                }
            } else { // unmatch types - the new value is not object
                Hunk::insert(map, path, Some(input));
            }
        }
    // object elements not null, objects nor array, present in this, may or may not be equal to input, so remove from input anyway
    } else if base != input {
        Hunk::insert(map, path, if input.is_null() { None } else { Some(input) });
    }
}


/// check remines on input object. extract all object if single field or element array
/// define the longest path to the unmatched object set or field or array
// TODO test case: [1,2,3] -> [,2,3] | [1,,3] as patch json: { "key.[0]": }
fn append(input: &Value, path: &Vec<DocIndex>, path_append: DocIndex, diff: &mut Vec<Hunk>) {
    let mut p= path.clone();
    p.push(path_append);
    match input {
        Value::Null => { } // do nothing
        Value::Array(a) => {
            if a.len() == 1 {
                if let Some(a) = a.get(0) {
                    append(a, &p, DocIndex::Idx(0), diff); // recursion with a new path
                }
            } else {
                Hunk::insert(diff, path, Some(input));
            }
        }
        Value::Object(i) => {
            if i.len() == 1 {
                if let Some((k, v)) = i.iter().next() {
                    append(v, &p, DocIndex::Name(k.clone()), diff); // recursion with a new path
                }
            } else {
                Hunk::insert(diff, path, Some(input));
            }
        }
        _ => {
            Hunk::insert(diff, path, Some(input));
        }
    }
}


impl Display for Mismatch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self.0).unwrap_or_else(|e| format!("ERROR: {}", e)))
    }
}

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
    fn test_ia_del() {
        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(0)], value: None},
                              &Hunk{path: vec![DocIndex::Idx(0)], value: None}));
        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(1)], value: None},
                              &Hunk{path: vec![DocIndex::Idx(0)], value: None}));
        assert!(is_intersect(&Hunk{path: vec![DocIndex::Idx(0)], value: None},
                              &Hunk{path: vec![DocIndex::Idx(1)], value: None}));

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


}