use std::cmp::{max, min};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use serde::{Deserialize, Serialize};
use crate::{txt, DocError, Intersection, MismatchDoc, MismatchDocCow, MismatchDocMut, Reason};

use crate::generic::{DocIndex, GenericValue, Hunk, HunkAction};
use crate::vec_processor::Range;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Mismatch(Vec<Hunk>);

impl MismatchDocMut<GenericValue> for Mismatch {
    fn apply_mut(&self, doc_root: &mut GenericValue, fail_fast: bool) -> Result<Vec<DocError>, DocError> {
        let mut err = Vec::new();
        for h in &self.0 {
            if let Err(e) = h.apply(doc_root) {
                if fail_fast {
                    return Err(e);
                } else {
                    err.push(e);
                }
            }
        }
        Ok(err)
    }
}

impl Hunk {
    fn apply(&self, doc_root: &mut GenericValue) -> Result<(), DocError> {
        let mut input = doc_root;  // current json node pointer
        // traverse the path
        for (idx, path) in self.path.iter().enumerate() {
            let last_element = idx == self.path.len() - 1;
            input = match path {
                DocIndex::Name(p) => {
                    if let GenericValue::Map(m) = input {
                        if last_element {
                            match &self.value {
                                HunkAction::Remove => {
                                    m.remove(p);
                                }
                                HunkAction::Update(v) => {
                                    m.insert(p.clone(), v.clone());
                                }
                                HunkAction::UpdateTxt(v) => {
                                    if let GenericValue::StringValue(s) = m.get(p)
                                            .ok_or_else(|| DocError::PathNotFound { path: DocIndex::Name(p.clone()) })? {
                                        let new_v = txt::Mismatch(v.clone()).apply(s)?;
                                        m.insert(p.clone(),  GenericValue::StringValue(new_v));
                                    } else {
                                        return Err(DocError::NotAString { path: DocIndex::Name(p.clone()) });
                                    }
                                }
                                HunkAction::Insert(v) => {
                                    m.insert(p.clone(), v.clone());
                                }
                                HunkAction::Swap(v)
                                | HunkAction::Clone(v) => {
                                    if let DocIndex::Name(vv) = v {
                                        let a = m.get(vv).ok_or_else(|| DocError::PathNotFound { path: DocIndex::Name(vv.clone()) })?;
                                        let x= m.insert(p.clone(), a.clone());
                                        if matches!(&self.value, HunkAction::Swap(_)) {
                                            if let Some(x) = x {
                                                m.insert(vv.clone(), x);
                                            } else {
                                                m.remove(vv);
                                            }
                                        }
                                    } else {
                                        return Err(DocError::IndexMismatch { index: v.clone() });
                                    }
                                }
                            }
                            return Ok(());
                        } else {
                            m.get_mut(p).ok_or_else(|| DocError::PathNotFound { path: DocIndex::Name(p.clone()) })?
                        }

                    } else {
                        return Err(DocError::NotAContainer { path: path.clone() });
                    }
                }
                DocIndex::Idx(p) => {
                    if let GenericValue::Array(m) = input {
                        if last_element {
                            match &self.value {
                                HunkAction::Remove => {
                                    m.remove(*p);
                                }
                                HunkAction::Update(v) => {
                                    m[*p] = v.clone();
                                }
                                HunkAction::UpdateTxt(v) => {
                                    if let GenericValue::StringValue(s) = m.get(*p)
                                            .ok_or_else(|| DocError::PathNotFound { path: DocIndex::Idx(*p) })? {
                                        let new_v = txt::Mismatch(v.clone()).apply(s)?;
                                        m[*p] = GenericValue::StringValue(new_v);
                                    } else {
                                        return Err(DocError::NotAString { path: DocIndex::Idx(*p) });
                                    }
                                }
                                HunkAction::Insert(v) => {
                                    m.insert(p.clone(), v.clone());
                                }
                                HunkAction::Swap(v)=> {
                                    if let DocIndex::Idx(vv) = v {
                                        swap(m, p, vv);
                                    } else {
                                        return Err(DocError::IndexMismatch { index: v.clone() });
                                    }
                                }
                                HunkAction::Clone(v) => {
                                    if let DocIndex::Idx(vv) = v {
                                        copy(m, *p, *vv);
                                    } else {
                                        return Err(DocError::IndexMismatch { index: v.clone() });
                                    }
                                }
                            }
                            return Ok(());
                        } else {
                            m.get_mut(*p).ok_or_else(|| DocError::PathNotFound { path: DocIndex::Idx(*p) })?
                        }
                    } else {
                        return Err(DocError::NotAContainer { path: path.clone() });
                    }
                }
            };
        }

        Ok(())
    }
}

fn copy(vec: &mut Vec<GenericValue>, destination_idx: usize, source_idx: usize) {
    let len = vec.len();
    if destination_idx > len || source_idx >= len {
        return;
    }
    if destination_idx == source_idx {
        return;
    }
    vec.insert(destination_idx, vec[source_idx].clone());
}

fn swap(vec: &mut Vec<GenericValue>, a: &usize, b: &usize) {
    let len = vec.len();
    if *a >= len || *b >= len {
        return;
    }
    if a == b {
        return;
    }
    vec.swap(*a, *b);
}


impl MismatchDoc<GenericValue> for Mismatch {
    fn new(base: &GenericValue, input: &GenericValue) -> Result<Self, DocError>
    where
        Self: Sized
    {
        Ok(Mismatch(GenericValue::diff(base, &input, &vec![])))
    }

    fn intersections(&self, input: &Self) -> Result<Vec<Intersection>, DocError> {

        let ranges_a = PathRange::new(&self.0);
        let ranges_b = PathRange::new(&input.0);

        let mut found = Vec::new();
        for (i, a) in self.0.iter().enumerate() {
            for (j, b) in input.0.iter().enumerate() {
                if let Some((reason, at)) = is_intersect(a, &ranges_a, b, &ranges_b, "a~b")
                    .or_else(|| is_intersect(b, &ranges_b, a, &ranges_a, "b~a")) {
                    found.push(Intersection { a: i, b: j, reason, at: pointer(&at) });
                }
            }
        }

        Ok(found)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug)]
struct PathRange {
    /// use as a key
    #[allow(unused)]
    path: Vec<DocIndex>,
    range: Range,
}

#[derive(Debug, PartialEq, Eq)]
struct PathKey (Vec<DocIndex>);

impl Hash for PathKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ".".hash(state);
        for (idx, p) in self.0.iter().enumerate() {
            if idx+1 == self.0.len() {
                if let DocIndex::Idx(_) = p {
                    break;
                }
            }
            p.hash(state);
        }
    }
}

type PathMapType = HashMap<PathKey, Vec<PathRange>>;
impl PathRange {

    fn new(input: &Vec<Hunk>) -> PathMapType {
        let mut ranges: PathMapType = HashMap::new();
        for op in input {
            debug_assert!(op.path.len() > 0, "invalid hunk with empty path: {:?}", op);
            if let DocIndex::Idx(idx) = op.path[op.path.len()-1] {
                let add = match op.value {
                    HunkAction::Insert(_) |
                    HunkAction::Clone(_) => true,
                    _ => false
                    // _ => { continue; }
                };
                let key = PathKey(op.path.clone());
                // insert into ranges
                ranges.get_mut(&key).map(|v| {
                    if v.len() == 0 || v[v.len() - 1].range.add != add {
                        v.push(PathRange { path: op.path.clone(), range: Range { start: idx, end: None, add } });
                    } else {
                        let mut found = false;
                        for r in v.iter_mut().rev() {
                            if r.range.add == add && r.range.end.is_none() {
                                r.range.end = Some(idx);
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            v.push(PathRange { path: op.path.clone(), range: Range { start: idx, end: None, add } });
                        }
                    }
                }).or_else(|| {
                    ranges.insert(key, vec![PathRange { path: op.path.clone(), range: Range { start: idx, end: None, add } }]);
                    Some(())
                });
            }
        }
        ranges
    }

    fn _overlap(&self, other: &Self) -> bool {
        self.range.overlap(&other.range)
    }


}

/// a path as a JSON pointer (RFC 6901): `/list/2/name`
fn pointer(path: &[DocIndex]) -> String {
    path.iter().map(|p| match p {
        DocIndex::Name(n) => format!("/{}", n.replace('~', "~0").replace('/', "~1")),
        DocIndex::Idx(i) => format!("/{}", i),
    }).collect()
}

/// check for intersection of two patches by path for update or delete of documents including vec/array:
/// why, and the path where
fn is_intersect(a: &Hunk, ranges_a: &PathMapType, b: &Hunk, ranges_b: &PathMapType, _msg: &str) -> Option<(Reason, Vec<DocIndex>)> {
    if a.path.len() == 0 || b.path.len() == 0 {
        return None; // assert no changes
    }

    // a change to a node is one to all inside it; a Swap or Clone reads its other
    // element too (b's in a's: the other call)
    for ta in touched(a) {
        for tb in touched(b) {
            if ta.len() < tb.len() && tb[..ta.len()] == ta[..] {
                return Some((Reason::Inside, ta));
            }
            if ta == tb && (ta != a.path || tb != b.path) {
                return Some((Reason::OtherElement, ta));
            }
        }
    }

    // this is a json path index, the longer path wont intersect with short one if longer do not contain the short
    let comp2idx = min(a.path.len(), b.path.len());

    // the reverse: check b in ranges_a will be in another call
    for i in 0..comp2idx {
        let cmp_val = a.path.len() == i+1 || b.path.len() == i+1;
        if !cmp_val {
            if a.path[i] != b.path[i]
            {
                return None; // diverged paths
            }
        }

        if let Some(cause) = is_intersect2(a, b, i, ranges_b) {
            #[cfg(feature="verbose")] println!("is_intersect {_msg} upto {comp2idx} as step {i} of {comp2idx} by: {cause:?}\nBase action: {a}\nInterfere with: {b}");
            return Some(cause);
        }
    }

    // check a's ranges in b's, of the array both are in
    let key = PathKey(a.path.clone());
    if let (Some(v), Some(x)) = (ranges_a.get(&key), ranges_b.get(&key)) {
        for r in x {
            for p in v {
                if p.range.overlap(&r.range) {
                    #[cfg(feature="verbose")] println!("is_intersect {_msg} as overlap {p:?} with {:?}", &r.range);
                    return Some((Reason::ArrayShift, a.path[..a.path.len() - 1].to_vec()));
                }
            }
        }
    }
    None
}

/// the paths a hunk changes or reads: its own, and a Swap's or Clone's other element
fn touched(h: &Hunk) -> Vec<Vec<DocIndex>> {
    let mut paths = vec![h.path.clone()];
    if let HunkAction::Swap(other) | HunkAction::Clone(other) = &h.value {
        let mut p = h.path.clone();
        if let Some(last) = p.last_mut() {
            *last = other.clone();
        }
        paths.push(p);
    }
    paths
}

/// check for intersection of two patches by path for update or delete of documents including vec/array
/// - why, and the path where: the array an element moves in, the node changed otherwise
fn is_intersect2(a: &Hunk, b: &Hunk, idx: usize, ranges_b: &PathMapType) -> Option<(Reason, Vec<DocIndex>)> {
    fn return_(cnd: bool, reason: Reason) -> Option<Reason> {
        if cnd {
            Some(reason)
        } else {
            None
        }
    }

    // check A in ranges_b
    if a.path.len() > 2 {
        if let DocIndex::Idx(idx) = a.path[a.path.len() - 1] {
            if let Some(ps) = ranges_b.get(&PathKey(a.path[..max(a.path.len() - 2, 0)].to_vec())) {
                for p in ps {
                    if p.range.start < idx && p.range.end.unwrap_or(usize::MAX) > idx {
                        return Some((Reason::ArrayShift, a.path[..a.path.len() - 1].to_vec()));
                    }
                }
            }
        }
    }
    if a.path.len() == idx+1 || b.path.len() == idx+1 {
        let reason = match &a.path[idx] {
            DocIndex::Name(a_path) => {
                match &b.path[idx] {
                    DocIndex::Name(b_path) =>
                        return_(a_path == b_path && &a.value != &b.value, Reason::SameNode),
                    DocIndex::Idx(_) =>
                        return_(a.value != b.value, Reason::NodeKind),
                }
            }
            DocIndex::Idx(a_idx) => {
                match &b.path[idx] {
                    DocIndex::Name(_) =>
                        return_(a.value != b.value, Reason::NodeKind),
                    DocIndex::Idx(b_idx) => {
                        match &a.value {
                            HunkAction::Remove => { // shift array left
                                match &b.value {
                                    HunkAction::Remove =>
                                        return_(a_idx != b_idx, Reason::ArrayShift),
                                    _ => return_(a_idx < b_idx, Reason::ArrayShift),
                                }
                            }
                            // shift array right, from a_idx on: the other's element there
                            // is the inserted one after, unless both insert it alike
                            HunkAction::Insert(a_val) => {
                                match &b.value {
                                    HunkAction::Insert(b_val) if a_idx == b_idx =>
                                        return_(a_val != b_val, Reason::SameNode),
                                    _ => return_(a_idx <= b_idx, Reason::ArrayShift),
                                }
                            }
                            // shift array right
                            HunkAction::Clone(a_val) => {
                                match &b.value {
                                    HunkAction::Clone(b_val) if a_idx == b_idx =>
                                        return_(a_val != b_val, Reason::SameNode),
                                    _ => return_(a_idx <= b_idx, Reason::ArrayShift),
                                }
                            }
                            // no shift actions below
                            HunkAction::Update(a_val) => {
                                match &b.value {
                                    HunkAction::Update(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, Reason::SameNode),
                                    HunkAction::UpdateTxt(_) =>
                                        return_(a_idx == b_idx, Reason::SameNode),
                                    HunkAction::Swap(_) =>
                                        return_(a_idx == b_idx, Reason::SameNode),
                                    // no shift compare to shift actions
                                    HunkAction::Remove =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                    // shift array right
                                    HunkAction::Insert(_) =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                    HunkAction::Clone(_) =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                }
                            }
                            // no shift
                            HunkAction::UpdateTxt(a_val) => {
                                match &b.value {
                                    HunkAction::UpdateTxt(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, Reason::SameNode),
                                    HunkAction::Update(_) =>
                                        return_(a_idx == b_idx, Reason::SameNode),
                                    HunkAction::Swap(_) =>
                                        return_(a_idx == b_idx, Reason::SameNode),
                                    // no shift compare to shift actions
                                    HunkAction::Remove =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                    // shift array right
                                    HunkAction::Insert(_) =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                    HunkAction::Clone(_) =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                }
                            }
                            // no shift; the elements of both indexes move with the other's shift
                            HunkAction::Swap(a_val) => {
                                let a_idx = match a_val {
                                    DocIndex::Idx(other) => max(a_idx, other),
                                    DocIndex::Name(_) => a_idx,
                                };
                                match &b.value {
                                    HunkAction::Swap(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, Reason::SameNode),
                                    HunkAction::UpdateTxt(_) =>
                                        return_(a_idx == b_idx, Reason::SameNode),
                                    HunkAction::Update(_) =>
                                        return_(a_idx == b_idx, Reason::SameNode),
                                    // no shift A, compare to shift B actions
                                    HunkAction::Remove =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                    // shift array right
                                    HunkAction::Insert(_) =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                    HunkAction::Clone(_) =>
                                        return_(a_idx > b_idx, Reason::ArrayShift),
                                }
                            }
                        }
                    }
                }
            }
        };
        reason.map(|r| match r {
            Reason::ArrayShift => (r, a.path[..idx].to_vec()),
            _ => (r, a.path[..=idx].to_vec()),
        })
    } else {
        None // values are non-comparable path state
    }
}


#[cfg(test)]
mod tests {
    use crate::generic::{from_json, from_str_vec, from_str_vec2, from_yaml, hs, to_json, to_yaml, NumericString};
    use super::*;

    #[test]
    fn parsing_test_1() {
        println!("--- JSON Example ---");
        let json_data = r#"
    {
        "name": "Rustacean",
        "age": 30,
        "is_cool": true,
        "projects": [
            "serde",
            "tokio"
        ],
        "details": {
            "version": 1.5
        }
    }"#;

        // 1. Parse the JSON data into our GenericValue.
        let mut generic_value = from_json(json_data).expect("Failed to parse JSON");
        println!("Original Parsed JSON: {:?}", generic_value);

        // 2. Modify the data using our custom structure.
        if let GenericValue::Map(ref mut map) = generic_value {
            // Update a value.
            if let Some(GenericValue::Numeric(age)) = map.get_mut("age") {
                *age = NumericString("31".to_string());
            }
            // Add a new key-value pair.
            map.insert("city".to_string(), GenericValue::StringValue("San Francisco".to_string()));
        }

        // 3. Serialize the modified data back into a JSON string.
        let modified_json = to_json(&generic_value).expect("Failed to serialize JSON");
        println!("Modified JSON:\n{}", modified_json);

        println!("\n--- YAML Example ---");
        let yaml_data = r#"
    name: "Rustacean"
    age: 30
    is_cool: true
    projects:
      - serde
      - tokio
    details:
      version: 1.6
    "#;
        let generic_yaml_value = from_yaml(yaml_data).expect("Failed to parse YAML");
        println!("Parsed YAML: {:?}", generic_yaml_value);
        let modified_yaml = to_yaml(&generic_yaml_value).expect("Failed to serialize YAML");
        println!("Serialized YAML:\n{}", modified_yaml);
        // Similarly for TOML and XML, using the respective functions.
    }


    #[test]
    fn parsing_test_2() {
        // --- Example 1: Deserializing a complex JSON object with mixed types ---
        let json_complex_data = r#"
    {
        "id": 12345678901234567890,
        "name": "Jane Doe",
        "is_active": true,
        "tags": ["alpha", "beta", "100000000000"],
        "metadata": {
            "version": 1.5,
            "nullable_field": null
        }
    }
    "#;

        let my_data_complex: GenericValue = serde_json::from_str(json_complex_data)
            .expect("Failed to deserialize complex data");
        let h = hs(&my_data_complex);
        println!("Deserialized complex data: {:?}\n hash: {h}", my_data_complex);

        println!("---");

        // --- Example 2: Deserializing a very large number (as a string) ---
        let json_numeric_string_data = r#"
    "123456789012345678901234567890"
    "#;

        let my_data_numeric_string: GenericValue = serde_json::from_str(json_numeric_string_data)
            .expect("Failed to deserialize numeric string data");

        let h = hs(&my_data_numeric_string);
        println!("Deserialized JSON numeric string: {:?}\n hash: {h}", my_data_numeric_string);
        // Expected output: Deserialized JSON numeric string: NumericId(NumericString("123456789012345678901234567890"))

        println!("---");

        // --- Example 3: Deserializing a standard string ---
        let json_string_data = r#"
    "some-string-label"
    "#;

        let my_data_string: GenericValue = serde_json::from_str(json_string_data)
            .expect("Failed to deserialize string data");
        let h = hs(&my_data_string);

        println!("Deserialized JSON string: {:?}\n hash: {h}", my_data_string);
        // Expected output: Deserialized JSON string: StringValue("some-string-label")

        println!("---");

        // --- Example 4: Deserializing a boolean ---
        let json_boolean_data = r#"
    true
    "#;

        let my_data_boolean: GenericValue = serde_json::from_str(json_boolean_data)
            .expect("Failed to deserialize boolean data");
        let h = hs(&my_data_boolean);

        println!("Deserialized JSON boolean: {:?}\n hash: {h}", my_data_boolean);
        // Expected output: Deserialized JSON boolean: Boolean(true)

        println!("---");

        // --- Example 5: Deserializing an array ---
        let json_array_data = r#"
    [
        123,
        "hello",
        false
    ]
    "#;

        let my_data_array: GenericValue = serde_json::from_str(json_array_data)
            .expect("Failed to deserialize array data");
        let h = hs(&my_data_array);

        println!("Deserialized JSON array: {:?}\n hash: {h}", my_data_array);
        // Expected output: Deserialized JSON array: Array([NumericId(NumericString("123")), StringValue("hello"), Boolean(false)])

        println!("---");

        // --- Example 6: Deserializing a null value ---
        let json_null_data = r#"
    null
    "#;

        let my_data_null: GenericValue = serde_json::from_str(json_null_data)
            .expect("Failed to deserialize null data");

        println!("Deserialized JSON null: {:?}", my_data_null);
        // Expected output: Deserialized JSON null: Null
    }

    #[test]
    fn test_intersect_map() {
        let base = from_str_vec2(vec![("a", "b"), ("c", "d")]);
        println!("Base: {:?}", base);
        let patch1 = Mismatch::new(&base, &from_str_vec2(vec![("a", "b1"), ("c", "d")])).unwrap();
        let patch2 = Mismatch::new(&base, &from_str_vec2(vec![("a", "b"), ("c", "d2")])).unwrap();
        let patch3 = Mismatch::new(&base, &from_str_vec2(vec![("a", "e"), ("c", "f")])).unwrap();
        println!("1~2 {:?}\n vs {:?}", patch1, patch2);
        assert!(!patch1.is_intersect(&patch2).unwrap(), "{:?} vs {:?}", patch1, patch2);
        println!("1~3 {:?}\n vs {:?}", patch1, patch3);
        assert!(patch1.is_intersect(&patch3).unwrap(), "{:?} vs {:?}", patch2, patch3);
        println!("2~3 {:?}\n vs {:?}", patch2, patch3);
        assert!(patch2.is_intersect(&patch3).unwrap(), "{:?} vs {:?}", patch2, patch3);
    }


    #[test]
    fn test_intersect_vec() {
        let base = from_str_vec(vec!["a", "b", "c"]);
        let patch1 = Mismatch::new(&base, &from_str_vec(vec!["a","b","d"])).unwrap();
        let patch2 = Mismatch::new(&base, &from_str_vec(vec!["a","f","c"])).unwrap();
        let patch3 = Mismatch::new(&base, &from_str_vec(vec!["a","f","e"])).unwrap();
        println!("1~2 {:?}\n vs {:?}", patch1, patch2);
        assert!(!patch1.is_intersect(&patch2).unwrap(), "{:?} vs {:?}", patch1, patch2);
        println!("\n1~3 {:?}\n vs {:?}", patch1, patch3);
        assert!(patch1.is_intersect(&patch3).unwrap(), "{:?} vs {:?}", patch2, patch3);
        println!("2~3 {:?}\n vs {:?}", patch2, patch3);
        assert!(patch2.is_intersect(&patch3).unwrap(), "{:?} vs {:?}", patch2, patch3);
    }


    #[test]
    fn test_intersect_vec2() {
        let base = from_str_vec(vec!["a", "b", "c"]);
        let patch1 = Mismatch::new(&base, &from_str_vec(vec!["a","b","d"])).unwrap();
        let patch2 = Mismatch::new(&base, &from_str_vec(vec!["a","f","c"])).unwrap();
        let patch3 = Mismatch::new(&base, &from_str_vec(vec!["a", "x", "b", "c"])).unwrap();
        println!("1~2 {:?}\n vs {:?}", patch1, patch2);
        assert!(!patch1.is_intersect(&patch2).unwrap(), "{:?} vs {:?}", patch1, patch2);
        println!("\n1~3 {:?}\n vs {:?}", patch1, patch3);
        assert!(patch1.is_intersect(&patch3).unwrap(), "{:?} vs {:?}", patch2, patch3);
        println!("2~3 {:?}\n vs {:?}", patch2, patch3);
        assert!(patch2.is_intersect(&patch3).unwrap(), "{:?} vs {:?}", patch2, patch3);
    }


}
