use std::collections::{HashMap, HashSet};
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
        let diffs = compare_strs(data1.as_str(), data2.as_str(), true, & [])
            .map_err(|e| DocParseError::new(e.to_string()))?;
        Ok(DocMismatch::from(diffs))
    }
    
    pub fn apply_json(&self, input: &mut serde_json::Value) -> Result<usize, DocParseError> {
        for (p, v) in &self.diff {
            if p.len() == 0 || p == "." {
                // root element
                
            }
            else {
                for pe in p.split(".") {
                    //
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
