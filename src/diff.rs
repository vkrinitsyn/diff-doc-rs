use std::cmp::{max, min};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use serde::{Deserialize, Serialize};
use crate::{txt, DocError, MismatchDoc, MismatchDocCow, MismatchDocMut};

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
                                            .ok_or_else(|| DocError::new(format!("Path not found: {}", p)))? {
                                        let new_v = txt::Mismatch(v.clone()).apply(s)?;
                                        m.insert(p.clone(),  GenericValue::StringValue(new_v));
                                    } else {
                                        return Err(DocError::new(format!("Expected string: {}", p)));
                                    }
                                }
                                HunkAction::Insert(v) => {
                                    m.insert(p.clone(), v.clone());
                                }
                                HunkAction::Swap(v)
                                | HunkAction::Clone(v) => {
                                    if let DocIndex::Name(vv) = v {
                                        let a = m.get(vv).ok_or_else(|| DocError::new(format!("Path not found: {}", p)))?;
                                        let x= m.insert(p.clone(), a.clone());
                                        if matches!(&self.value, HunkAction::Swap(_)) {
                                            if let Some(x) = x {
                                                m.insert(vv.clone(), x);
                                            } else {
                                                m.remove(vv);
                                            }
                                        }
                                    } else {
                                        return Err(DocError::new(format!("index type must match: {:?}", v)));
                                    }
                                }
                            }
                            return Ok(());
                        } else {
                            m.get_mut(p).ok_or_else(|| DocError::new(format!("Path not found: {}", p)))?
                        }

                    } else {
                        return Err(DocError::new(format!("Path index not found: {}", p)));
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
                                            .ok_or_else(|| DocError::new(format!("Path not found: {}", p)))? {
                                        let new_v = txt::Mismatch(v.clone()).apply(s)?;
                                        m[*p] = GenericValue::StringValue(new_v);
                                    } else {
                                        return Err(DocError::new(format!("Expected string field: {}", p)));
                                    }
                                }
                                HunkAction::Insert(v) => {
                                    m.insert(p.clone(), v.clone());
                                }
                                HunkAction::Swap(v)=> {
                                    if let DocIndex::Idx(vv) = v {
                                        swap(m, p, vv);
                                    } else {
                                        return Err(DocError::new(format!("index type must match: {:?}", v)));
                                    }
                                }
                                HunkAction::Clone(v) => {
                                    if let DocIndex::Idx(vv) = v {
                                        copy(m, *p, *vv);
                                    } else {
                                        return Err(DocError::new(format!("index type must match: {:?}", v)));
                                    }
                                }
                            }
                            return Ok(());
                        } else {
                            m.get_mut(*p).ok_or_else(|| DocError::new(format!("Path not found: {}", p)))?
                        }
                    } else {
                        return Err(DocError::new(format!("Path index not found: {}", p)));
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

    fn is_intersect(&self, input: &Self) -> Result<bool, DocError> {

        let ranges_a = PathRange::new(&self.0);
        let ranges_b = PathRange::new(&input.0);

        for a in &self.0 {
            for b in &input.0 {
                if is_intersect(a, &ranges_a, b, &ranges_b, #[cfg(debug_assertions)] "a~b") {
                    return Ok(true);
                }

                if is_intersect(b, &ranges_b, a, &ranges_a, #[cfg(debug_assertions)] "b~a") {
                    return Ok(true);
                }
            }
        }

        Ok(false)
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

/// check for intersection of two patches by path for update or delete of documents including vec/array
fn is_intersect(a: &Hunk, ranges_a: &PathMapType, b: &Hunk, ranges_b: &PathMapType, #[cfg(debug_assertions)] _msg: &str) -> bool {
    if a.path.len() == 0 || b.path.len() == 0 {
        return false; // assert no changes
    }

    // this is a json path index, the longer path wont intersect with short one if longer do not contain the short
    let comp2idx = min(a.path.len(), b.path.len());

    // the reverse: check b in ranges_a will be in another call
    for i in 0..comp2idx {
        let cmp_val = a.path.len() == i+1 || b.path.len() == i+1;
        if !cmp_val {
            if a.path[i] != b.path[i]
            {
                return false; // diverged paths
            }
        }

        if let Some(_cause) = is_intersect2(a, b, i, ranges_b) {
            #[cfg(feature="verbose")] println!("is_intersect {_msg} upto {comp2idx} as step {i} of {comp2idx} by: {_cause}\nBase action: {a}\nInterfere with: {b}");
            return true;
        }
    }

    // check ranges_a in ranges_b
    for (k, v) in ranges_a {
        if let Some(x) = ranges_b.get(k) {
            for r in x {
                for p in v {
                    if p.range.overlap(&r.range) {
                        #[cfg(feature="verbose")] println!("is_intersect {msg} as overlap {p:?} with {:?}", &r.range);
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// check for intersection of two patches by path for update or delete of documents including vec/array
fn is_intersect2(a: &Hunk, b: &Hunk, idx: usize, ranges_b: &PathMapType) -> Option<&'static str> {
    fn return_(cnd: bool, msg: &'static str) -> Option<&'static str> {
        if cnd {
            Some(msg)
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
                        return Some("overlap in ranges");
                    }
                }
            }
        }
    }
    if a.path.len() == idx+1 || b.path.len() == idx+1 {
        match &a.path[idx] {
            DocIndex::Name(a_path) => {
                match &b.path[idx] {
                    DocIndex::Name(b_path) =>
                        return_(a_path == b_path && &a.value != &b.value, "diff values"),
                    DocIndex::Idx(_) =>
                        return_(a.value != b.value, "discrepancy in types name-idx, but in case of delete - no matter"),
                }
            }
            DocIndex::Idx(a_idx) => {
                match &b.path[idx] {
                    DocIndex::Name(_) =>
                        return_(a.value != b.value, "discrepancy in types idx-name, but in case of delete - no matter"),
                    DocIndex::Idx(b_idx) => {
                        match &a.value {
                            HunkAction::Remove => { // shift array left
                                match &b.value {
                                    HunkAction::Remove =>
                                        return_(a_idx != b_idx, "both removes with different indexes"),
                                    _ => return_(a_idx < b_idx, "expected to remove but another lower index action found"),
                                }
                            }
                            // shift array right
                            HunkAction::Insert(a_val) => {
                                match &b.value {
                                    HunkAction::Insert(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, "both insert with different values"),
                                    _ => return_(a_idx < b_idx, "expected to insert but another lower index action found"),
                                }
                            }
                            // shift array right
                            HunkAction::Clone(a_val) => {
                                match &b.value {
                                    HunkAction::Clone(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, "both clone with different values"),
                                    _ => return_(a_idx < b_idx, "expected to clone but another lower index action found"),
                                }
                            }
                            // no shift actions below
                            HunkAction::Update(a_val) => {
                                match &b.value {
                                    HunkAction::Update(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, "both update with different values"),
                                    HunkAction::UpdateTxt(_) =>
                                        return_(a_idx == b_idx, "update with other update txt"),
                                    HunkAction::Swap(_) =>
                                        return_(a_idx == b_idx, "update with other swap"),
                                    // no shift compare to shift actions
                                    HunkAction::Remove =>
                                        return_(a_idx > b_idx, "update with other remove at lower index"),
                                    // shift array right
                                    HunkAction::Insert(_) =>
                                        return_(a_idx > b_idx, "update with other insert at lower index"),
                                    HunkAction::Clone(_) =>
                                        return_(a_idx > b_idx, "update with other clone at lower index"),
                                }
                            }
                            // no shift
                            HunkAction::UpdateTxt(a_val) => {
                                match &b.value {
                                    HunkAction::UpdateTxt(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, "both update with different values"),
                                    HunkAction::Update(_) =>
                                        return_(a_idx == b_idx, "update txt with other update"),
                                    HunkAction::Swap(_) =>
                                        return_(a_idx == b_idx, "update txt  with other swap"),
                                    // no shift compare to shift actions
                                    HunkAction::Remove =>
                                        return_(a_idx > b_idx, "update txt with other insert at lower index"),
                                    // shift array right
                                    HunkAction::Insert(_) =>
                                        return_(a_idx > b_idx, "update txt with other insert at lower index"),
                                    HunkAction::Clone(_) =>
                                        return_(a_idx > b_idx, "update txt with other clone at lower index"),
                                }
                            }
                            // no shift
                            HunkAction::Swap(a_val) => {
                                match &b.value {
                                    HunkAction::Swap(b_val) =>
                                        return_(a_idx == b_idx && a_val != b_val, "both swap with different values"),
                                    HunkAction::UpdateTxt(_) =>
                                        return_(a_idx == b_idx, "swap with other txt update"),
                                    HunkAction::Update(_) =>
                                        return_(a_idx == b_idx, "swap with other update"),
                                    // no shift A, compare to shift B actions
                                    HunkAction::Remove =>
                                        return_(a_idx > b_idx, "swap with other remove at lower index"),
                                    // shift array right
                                    HunkAction::Insert(_) =>
                                        return_(a_idx > b_idx, "swap with other insert at lower index"),
                                    HunkAction::Clone(_) =>
                                        return_(a_idx > b_idx, "swap with other clone at lower index"),
                                }
                            }
                        }
                    }
                }
            }
        }
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
