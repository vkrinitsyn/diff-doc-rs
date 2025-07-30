use std::collections::{HashMap, HashSet};
use serde_json::{Map, Value};
use crate::{DocError, MismatchDoc};

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mismatch (HashMap<String, Option<Value> >);

impl MismatchDoc<Value> for crate::json::Mismatch {
    fn new(base: &Value, input: &Value) -> Result<Self, DocError>
    where
        Self: Sized
    {
        let mut diff = HashMap::new();
        let mut input = input.clone();
        let _ = jdiff(base, &mut input, ".".to_string(), 0, &mut diff)?;
        let _ = append(&mut input, &mut diff)?;
        Ok(Mismatch(diff))
    }

    fn apply_to(&self, base: &Value) -> Result<Value, DocError> {
        todo!()
    }

    fn is_intersect(&self, input: &Self) -> Result<bool, DocError> {
        todo!()

        // impl<T> MismatchType<T> for Sized {
        /*
            pub fn is_intersect<T>(a: &impl MismatchDoc<T>, b: &impl MismatchDoc<T>) -> Result<bool, DocParseError> {
                if discriminant(a) != discriminant(b) {
                    return Err(DocParseError::new("wrong type compare"));
                }
        
                if let Mismatches::Patch(a) = &a {
                    if let Mismatches::Patch(b) = &b {
                        return is_intersect_txt(a, b).map_err(|e| DocParseError::new(e.to_string()));
                    }
                }
        
        
                for (a, v) in &a {
                    if b.is_partial_contains(a, v)? {
                        return Ok(true);
                    }
                }
        
                for (b, v) in &b {
                    if a.is_partial_contains(b, v)? {
                        return Ok(true);
                    }
                }
        
                Ok(false)
            }
        */

        // fn is_partial_contains<T>(&self, path: &String, val: &Option<T>) -> Result<bool, DocParseError> {
        //     todo!()
        // }
        /*
        
            fn is_partial_contains(&self, path: &String, val: &Option<T>) -> Result<bool, DocParseError> {
                if path.ends_with("]") { // it's array element update
                    match path.rfind("[") {
                        None => Err(DocParseError::new(format!("expected array, but found {}", path))), 
                        Some(idx) => {
                            let idx = path[idx + 1..path.len() - 2].parse::<usize>()
                                .map_err(|e| DocParseError::new(format!("expected array numeric index, but {}", e)))?;
        
                            Ok(self.diff.iter()
                                .find(|(p, v)| if p.ends_with("]") {
                                    match p.rfind("[") {
                                        None => { false } // wrong format actually
                                        Some(pidx) => match p[pidx + 1..p.len() - 2].parse::<usize>() {
                                            Ok(pidx) => {
                                                if val.is_none() && v.is_none() {
                                                    idx != pidx // same delete operation is mutually overlap, unless delete same index
                                                } else if val.is_some() && v.is_some() {
                                                    idx == pidx  // same index update is overlap
                                                } else if val.is_none() {
                                                    idx > pidx   // only one delete this idx, should be 
                                                } else {
                                                    false
                                                }
                                            }
                                            Err(_) => { false } // wrong format actually
                                           }
                                        }
                                  } else { 
                                      false 
                                  }
                              )
                              .is_some())
                                  // None => Ok(false), // same deletion
                      }
                  }  
                } else {
                    if let Mismatch::Text = self.mismatch_type {
                        //
                    }
                    // let p = self.diff.get(path);
                    let base = self.diff.get(path).map(|v| v != val);
                    Ok(base.unwrap_or(false)
                        || self.diff.iter().find(|(s, v)|
                            path.starts_with(*s) && v!=&val).is_some()) // same key not found, but portion?)
                }
            }
        */
    }
}


/// define the longest path to the unmatched object set or field or array
fn append(input: &mut Value, diff: &mut HashMap<String, Option<Value>>) -> Result<(), DocError> {
    todo!()

    // Ok(())
}

/// traverse to all json tree and clean the input intersect with base, so remining input will be added to discrepancy
/// return the tree is empty
fn jdiff(base: &Value, input: &mut Value, path: String, index: usize, diff: &mut HashMap<String, Option<Value>>) -> Result<bool, DocError> {
    Ok(if base.is_null() && !input.is_null() {
        diff.insert(path, Some(input.to_owned()));
        true
    } else if !base.is_null() && input.is_null() {
        diff.insert(path, None);
        true
    } else if base.is_array() {
        if let Some(b) = base.as_array() {
            if let Some(i) = input.as_array() {
                for val in 0..b.len() {
                    //
                }
            } else {
                // unmatched sides - input is not array

            }
        }
        input.as_array().map(|v| v.len()).unwrap_or(0) == 0
    } else if base.is_object() {
        if let Some(b) = base.as_object() {
            let mut empty = true;
            for (key,val) in b {
                //
                empty = empty && match input.as_object_mut() {
                    None => {
                        if input.is_null() {
                            true // to remove key
                        } else {
                            // unmatched sides
                            false
                        }
                    }
                    Some(i) => {
                        if match i.get_mut(key) {
                            None => {
                                true
                            }
                            Some(input) => {
                                jdiff(val, input, format!("{}.{}", path, key), index, diff)?
                            }
                        } {
                            i.remove(key); // remove object if empty
                        }
                        i.is_empty()
                    }
                };
            }
            empty
        } else {
            // wont happens
            false
        }
    } else if base != input {
        diff.insert(path, if input.is_null() { None } else { Some(input.to_owned()) });
        true
    } else {
        true
    })
}



// fn jts(input: &Value) -> Result<String, DocError> {
//     serde_json::to_string(&input).map_err(|e| DocError::new(e.to_string()))
// }
