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
                                    m.insert(p.clone(), v.clone());
                                }
                                HunkAction::UpdateTxt(v) => {
                                    if let GenericValue::StringValue(s) = m.get(*p)
                                            .ok_or_else(|| DocError::new(format!("Path not found: {}", p)))? {
                                        let new_v = txt::Mismatch(v.clone()).apply(s)?;
                                        m.insert(p.clone(),  GenericValue::StringValue(new_v));
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
            if input.0.iter().find_map(|b| Some( is_intersect(a, &ranges_a, b, &ranges_b) || is_intersect(b, &ranges_b, a, &ranges_a) ))
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

struct PathRange {
    path: Vec<DocIndex>,
    range: Range,
}

#[derive(Debug, PartialEq, Eq)]
struct PathKey (Vec<DocIndex>);

impl Hash for PathKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ".".hash(state);
        for (idx, p) in self.0.iter().enumerate() {
            if idx == self.0.len() - 2 {
                break;
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
            if let DocIndex::Idx(idx) = op.path[op.path.len()-1] {
                let add = match op.value {
                    HunkAction::Remove => false,
                    HunkAction::Insert(_) |
                    HunkAction::Clone(_) => true,
                    _ => { continue; }
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
        ranges//.into_values().flatten().collect()
    }

    fn overlap(&self, other: &Self) -> bool {
        self.range.overlap(&other.range)
    }


}

/// check for intersection of two patches by path for update or delete of documents including vec/array
fn is_intersect(a: &Hunk, ranges_a: &PathMapType, b: &Hunk, ranges_b: &PathMapType) -> bool {
    if a.path.len() == 0 || b.path.len() == 0 {
        return false; // assert changes
    }

    // this is a json path index, the longer path wont intersect with short one if longer do not contain the short
    let comp2idx = min(a.path.len(), b.path.len());

    // the reverse: check b in ranges_a will be in another call
    for i in 0..comp2idx {
        if let Some(cause) = is_intersect2(a, b, i, !(a.path.len()==b.path.len() && i==comp2idx), ranges_b) {
            #[cfg(debug_assertions)] println!("is_intersect as step {i} of {comp2idx} by: {cause}\n{a}\n{b}");
        } else {
            return false;
        }
    }

    // check ranges_a in ranges_b
    for (k, v) in ranges_a {
        if let Some(x) = ranges_b.get(k) {
            for r in x {
                for p in v {
                    if p.range.overlap(&r.range) {
                        return true;
                    }
                }
            }
        }
    }
    true
}

/// check for intersection of two patches by path for update or delete of documents including vec/array
fn is_intersect2(a: &Hunk, b: &Hunk, idx: usize, ignore_val: bool, ranges_b: &PathMapType) -> Option<&'static str> {
    // check a in ranges_b
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

    match &a.path[idx] {
        DocIndex::Name(a_path) => {
            match &b.path[idx] {
                DocIndex::Name(b_path) => {
                    if a_path == b_path && (ignore_val || &a.value != &b.value) {
                        Some("diff values ")
                    } else { None }
                }
                DocIndex::Idx(_) => {
                    if !ignore_val || a.value != b.value {
                        Some("discrepancy in types name-idx, but in case of delete - no matter")
                    } else { None }
                }
            }
        }
        DocIndex::Idx(a_idx) => {
            match &b.path[idx] {
                DocIndex::Name(_) => {
                    if !ignore_val || a.value != b.value {
                        Some("discrepancy in types idx-name, but in case of delete - no matter")
                    } else { None }
                }
                DocIndex::Idx(b_idx) => {
                    match &a.value {

                        HunkAction::Remove => {
                            match &b.value {
                                HunkAction::Remove => if a_idx != b_idx && !ignore_val {
                                    Some("both removes with different indexes")
                                } else { None
                                },
                                _ => Some("expected to remove but another action found")
                            }
                        }
                        HunkAction::Update(a_val) => {
                            match &b.value {
                                HunkAction::Update(b_val) =>
                                    if a_idx == b_idx && a_val != b_val && !ignore_val {
                                        Some("both update with different values")
                                    } else { None },
                                _ => Some("expected to update but another action found")
                            }
                        }
                        HunkAction::UpdateTxt(a_val) => {
                            match &b.value {
                                HunkAction::UpdateTxt(b_val) =>
                                    if a_idx == b_idx && a_val != b_val && !ignore_val {
                                        Some("both update with different values")
                                    } else { None },
                                _ => Some("expected to remove but another action found")
                            }
                        }
                        HunkAction::Insert(a_val) => {
                            match &b.value {
                                HunkAction::Insert(b_val) =>
                                    if a_idx == b_idx && a_val != b_val && !ignore_val {
                                        Some("both insert with different values")
                                    } else { None },
                                _ => Some("expected to insert but another action found")
                            }
                        }
                        HunkAction::Swap(a_val) => {
                            match &b.value {
                                HunkAction::Swap(b_val) =>
                                    if a_idx == b_idx && a_val != b_val && !ignore_val {
                                        Some("both swap with different values")
                                    } else { None },
                                _ => Some("expected to swap but another action found")
                            }
                        }
                        HunkAction::Clone(a_val) => {
                            match &b.value {
                                HunkAction::Clone(b_val) =>
                                    if a_idx == b_idx && a_val != b_val && !ignore_val {
                                        Some("both clone with different values")
                                    } else { None },
                                _ => Some("expected to clone but another action found")
                            }
                        }
                    }
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::generic::{from_json, from_yaml, hs, to_json, to_yaml, NumericString};
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

}
