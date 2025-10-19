use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::{DefaultHasher, Hash, Hasher};
use serde::{Deserialize, Deserializer, Serialize};
use crate::map_processor::min_map_changes;
use crate::txt::DiffOp;
use crate::vec_processor::compute_vec_diff;

#[derive(Clone, Serialize, Deserialize, Debug,  PartialEq, Eq)]
#[serde(untagged)]
pub enum GenericValue {

    // A number, which is converted to a string.
    Numeric(NumericString),

    // A regular JSON object.
    Map(HashMap<String, GenericValue>),

    // A regular JSON array.
    Array(Vec<GenericValue>),

    // A boolean value.
    Boolean(bool),

    // A string value. This should be last to avoid matching numbers.
    StringValue(String),

    // Represents a null value.
    Null,
}



// Manually implement `Hash` for MyData.
// This is necessary because `HashMap` does not implement `Hash` in Rust's standard library.
// To make it deterministic, we sort the key-value pairs before hashing.
impl Hash for GenericValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            GenericValue::Map(map) => {
                // Collect keys and sort them to ensure a deterministic hash.
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort_unstable();

                // Hash the number of entries and then each sorted key-value pair.
                map.len().hash(state);
                for key in keys {
                    key.hash(state);
                    map.get(key).unwrap().hash(state);
                }
            }
            GenericValue::Array(arr) => arr.hash(state),
            GenericValue::Numeric(num) => num.hash(state),
            GenericValue::Boolean(b) => b.hash(state),
            GenericValue::StringValue(s) => s.hash(state),
            GenericValue::Null => 0.hash(state), // A simple hash for a null value
        }
    }
}

pub(crate) fn hs<T: Hash>(input: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let h = hasher.finish();
    // #[cfg(debug_assertions)] println!("{input} = {h}");
    h
}


#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DocIndex {
    #[serde(rename ="n")]
    Name(String),
    #[serde(rename ="i")]
    Idx(usize)
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// value of changes
pub enum HunkAction {
    /// remove array element or map or document node
    Remove,

    /// update element to a new value
    Update(GenericValue),

    /// used only for large text fields, to store text diff operations
    UpdateTxt(Vec<DiffOp>),

    /// update element of a document to a new value, same as Update for map DocIndex, but insert a new for array's DocIndex
    Insert(GenericValue),

    /// use next Remove if need to delete the old position
    /// DocIndex must match the type of element at path
    Swap(DocIndex),

    /// do insert with shift right
    /// DocIndex must match the type of element at path
    Clone(DocIndex),
}

impl HunkAction {
    pub(crate) fn is_update(&self) -> bool {
        matches!(self, HunkAction::Update(_) | HunkAction::UpdateTxt(_))
    }

}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// chunk of changes
pub struct Hunk {
    /// path to an element to operate with
    #[serde(rename ="p")]
    pub(crate) path: Vec<DocIndex>,
    /// command to handle
    #[serde(rename ="v")]
    pub(crate) value: HunkAction,
}

impl Hunk {
    pub(crate) fn append(diff: &mut Vec<Hunk>, path: &Vec<DocIndex>, current: DocIndex, value: HunkAction) {
        let mut path = path.clone();
        path.push(current);
        diff.push(Self { path, value })
    }
}

impl Display for Hunk {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self).unwrap_or_else(|e| format!("ERROR: {}", e)))
    }
}


impl GenericValue {
    fn as_update(&self, path: &Vec<DocIndex>) -> Vec<Hunk> {
        vec![Hunk { path: path.clone(), value: HunkAction::Update(self.clone()) }]
    }

    pub(crate) fn chars(&self) -> Vec<char> {
        match &self {
            GenericValue::StringValue(v) => v.chars().collect(),
            _ => vec![]
        }
    }

    /// identify minimum changes
    pub fn diff(base: &Self, input: &Self, path: &Vec<DocIndex>) -> Vec<Hunk> {
        match base {
            GenericValue::Numeric(a) => {
                if let GenericValue::Numeric(b) = input {
                    if a != b {
                        input.as_update(path)
                    } else {
                        vec![]
                    }
                } else {
                    input.as_update(path)
                }
            }
            GenericValue::Map(a) => {
                if let GenericValue::Map(b) = input {
                    min_map_changes(a, b, path)
                } else {
                    input.as_update(path)
                }
            }
            GenericValue::Array(a) => {
                if let GenericValue::Array(b) = input {
                    compute_vec_diff(a, b, path)
                } else {
                    input.as_update(path)
                }
            }
            GenericValue::Boolean(a) => {
                if let GenericValue::Boolean(b) = input {
                    if a != b {
                        input.as_update(path)
                    } else {
                        vec![]
                    }
                } else {
                    input.as_update(path)
                }
            }
            GenericValue::StringValue(a) => {
                if let GenericValue::StringValue(b) = input {
                    if a != b {
                        if a.len() + b.len() > (u16::MAX >> 4) as usize {
                            // use text diff
                            let ops = crate::txt::diff(a, b);
                            vec![Hunk { path: path.clone(), value: HunkAction::UpdateTxt(ops) }]
                        } else {
                            input.as_update(path)
                        }
                        // input.as_update(path)
                    } else {
                        vec![]
                    }
                } else {
                    input.as_update(path)
                }
            }
            GenericValue::Null => {
                if let GenericValue::Null = input {
                    vec![]
                } else {
                    input.as_update(path)
                }
            }
        }
    }
}

// --- Parsing Functions ---
pub fn from_str_vec(s: Vec<&str>) -> GenericValue {
    GenericValue::Array(s.into_iter().map(|v| GenericValue::StringValue(v.to_string())).collect())
}

pub fn from_json(s: &str) -> Result<GenericValue, serde_json::Error> {
    serde_json::from_str(s)
}

pub fn from_yaml(s: &str) -> Result<GenericValue, serde_yaml::Error> {
    serde_yaml::from_str(s)
}

pub fn from_toml(s: &str) -> Result<GenericValue, toml::de::Error> {
    toml::from_str(s)
}

pub fn from_xml(s: &str) -> Result<GenericValue, serde_xml_rs::Error> {
    serde_xml_rs::from_str(s)
}

// --- Serialization Functions ---
pub fn to_json(value: &GenericValue) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

pub fn to_yaml(value: &GenericValue) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(value)
}

pub fn to_toml(value: &GenericValue) -> Result<String, toml::ser::Error> {
    toml::to_string_pretty(value)
}

pub fn to_xml(value: &GenericValue) -> Result<String, serde_xml_rs::Error> {
    serde_xml_rs::to_string(value)
}




// This is a new type that represents a string that must contain a number.
// We implement a custom `Deserialize` trait to enforce this.
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub struct NumericString(pub String);

// Manually implement `Serialize` for NumericString.
// This is necessary to ensure that numbers are serialized as JSON numbers, not strings.
impl Serialize for NumericString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Attempt to parse the inner string as a u128. This will handle large numbers.
        if let Ok(num) = self.0.parse::<u128>() {
            // If it's a number, serialize it as a number.
            serializer.serialize_u128(num)
        } else if let Ok(num) = self.0.parse::<i128>() {
            serializer.serialize_i128(num)
        } else if let Ok(num) = self.0.parse::<f64>() {
            serializer.serialize_f64(num)
        } else {
            // If it's not a valid number, serialize it as a string.
            serializer.serialize_str(&self.0)
        }
    }
}


impl<'de> Deserialize<'de> for NumericString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde_json::Value;
        // First, deserialize the incoming value as a generic `serde_json::Value`.
        // This allows us to handle both JSON numbers and strings.
        let value = Value::deserialize(deserializer)?;

        // Then, match on the type of the value to handle both cases.
        let s = match value {
            // If the value is a number, convert it to a string.
            Value::Number(num) => num.to_string(),
            // If the value is a string, check if it's a valid number.
            Value::String(s) => {
                if s.parse::<u128>().is_ok()
                || s.parse::<i128>().is_ok()
                || s.parse::<f64>().is_ok()
                {
                    s
                } else {
                    return Err(serde::de::Error::custom("string is not a valid number"));
                }
            }
            // For any other type, return an error.
            _ => {
                return Err(serde::de::Error::custom(
                    "data did not match any numeric or string variant"
                ));
            }
        };

        Ok(NumericString(s))
    }
}

/*
impl MismatchDocMut<GenericValue> for Mismatch {
    fn apply_mut(&self, input: &mut GenericValue) -> Result<(), DocError> {
        for h in &self.0 {
            match &h.value {
                /*
                None => {
                    self.remove(&h.path, input);
                }
                Some(v) => {
                    self.modify(&h.path, v.clone(), input);
                }
                */
                HunkAction::Remove => {}
                HunkAction::Update(_) => {}
                HunkAction::UpdateString(_) => {}
                HunkAction::Replace(_, _) => {}
                HunkAction::Clone(_) => {}
            }
        }
        Ok(())
    }
}


//


impl Mismatch {

    fn remove(&self, path: &Vec<DocIndex>, json_root: &mut GenericValue) {
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

    fn modify(&self, path: &Vec<DocIndex>, value: GenericValue, json_root: &mut GenericValue) {
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
                        *input = GenericValue::Array(repeat(GenericValue::Null.clone()).take(*idx + 1).collect());
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


impl MismatchDoc<GenericValue> for Mismatch {
    fn new(base: &GenericValue, input: &GenericValue) -> Result<Self, DocError>
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
*/
#[cfg(test)]
mod tests {
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
