use std::collections::{HashMap, HashSet};
use std::num::ParseIntError;
use serde_json::json;
// use json_diff_ng::{compare_strs, Mismatch};
use crate::{DocMismatch, DocParseError, MismatchType};
use crate::dd::enums::DiffType;
use crate::dd::mismatch::Mismatch;
use crate::dd::process::compare_strs;

impl From<Mismatch> for DocMismatch {
    fn from(value: Mismatch) -> Self {
        let mut diff = HashMap::new();
        for (dt, dv) in value.all_diffs() {
            let path = &dv.path.iter()
                .map(|row| format!(".{}", row))
                .collect::<String>();
            diff.insert(path.to_owned(),
                        match dt {
                            DiffType::LeftExtra => None,
                            _ => dv.value.map(|x | format!("{}", x))
                        });
        }
        DocMismatch {mismatch_type: MismatchType::Json, diff}
    }
}

impl DocMismatch {
    pub fn new_json(data1: &String, data2: &String) -> Result<Self, DocParseError> {
        let diffs = compare_strs(data1.as_str(), data2.as_str(), false, & [])
            .map_err(|e| DocParseError::new(e.to_string()))?;
        Ok(DocMismatch::from(diffs))
    }
    
    /// idempotent patch apply
    pub fn apply_json(&self, input: &mut serde_json::Value) -> Result<usize, DocParseError> {
        fn clean(input: &mut serde_json::Value) {
            if input.is_array() {
                *input = json!([]);
            } else {
                *input = json!({});
            }
        }
        let curr: &mut serde_json::Value = input; 
        for (p, v) in &self.diff {
            if p.len() == 0 || p == "." {
                // root element
                match v {
                    None => clean(input),
                    Some(v) => {
                        if v.len() == 0 {
                            clean(input);
                        } else {
                            *input = serde_json::from_str(v.as_str())
                                .map_err(|e| DocParseError::new(e.to_string()))?;
                        }
                    }
                }
                break;
            } else {
                for pe in p.split(".") {
                    // current element is an array
                    if pe.starts_with("[") && pe.starts_with("]") {
                        match pe[1..pe.len()-2].parse::<usize>() {
                            Ok(idx) => {
                                if curr.is_array() || (idx == 0 && curr.is_null()) {
                                    let current_size = curr.as_array().map(|a| a.len()).unwrap_or(0);
                                    match v {
                                        None => { // remove
                                            if idx < current_size {
                                                curr.as_array_mut()
                                                    .and_then(|a| {  a.remove(idx); Some(1)});
                                            }
                                        }
                                        Some(v) => {
                                            if idx == 0 && curr.is_null() { // new arrays with single element
                                                *curr = json!([v]);
                                            } else if idx > current_size - 1 {
                                                return Err(DocParseError::new(format!("type mismatch: expected array size of {} but {}", idx+1, current_size)));
                                            } else if idx == current_size - 1 { // append existing array
                                                curr.as_array_mut()
                                                    .and_then(|a| {  a.push(json!(v)); Some(1)});
                                            } else { // update arrays element by index
                                                curr.as_array_mut()
                                                    .and_then(|a| {  a[idx] = json!(v); Some(1)});
                                            }
                                        }
                                    }

                                } else {
                                    return Err(DocParseError::new(format!("type mismatch: {} expected array", p)));
                                }
                            }
                            Err(e) => {
                                return Err(DocParseError::new(e.to_string()));
                            }
                        }
                    } 
                }
            }
            // input.as_array()
            match v {
                None => {
                    // remove element
                }
                Some(_) => {
                    // update
                }
            }
        }
        
        todo!()
    }
    
    pub fn apply_json_str(&self, input: &String) -> Result<String, DocParseError> {
        let mut j: serde_json::Value = serde_json::from_str(input.as_str())
            .map_err(|e| DocParseError::new(e.to_string()))?;
        
        let _ = self.apply_json(&mut j)?;
        
        serde_json::to_string_pretty(&j)
               .map_err(|e| DocParseError::new(e.to_string()))
    }
}
