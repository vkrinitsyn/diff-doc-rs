use std::cmp::{max, min};
use std::collections::{BTreeMap, HashMap, HashSet};
use crate::{DocMismatch, DocParseError, MismatchType};


impl DocMismatch {
    pub fn new_txt(base: &String, input: &String) -> Result<Self, DocParseError> {
        let mut diff = HashMap::new();
        let base: Vec<&str> = base.split("\n").collect();
        let a: Vec<&str> = input.split("\n").collect();
        /*
        for i in min(base.len(), a.len())..max(base.len(), a.len())-1 {
            if if base.len() > a.len() {
                base[i].len() > 0
            } else {
                a[i].len() > 0
            } {
                return Err(DocParseError::new("unsupported text sizes. Expected to be equal lines count. Trailing empty lines is ignored."));
            }
        }
         */
        for i in 0..max(base.len(), a.len())   {
            if i >= a.len() {
                diff.insert(format!("{}", i), None); // delete line
            } else if i >= base.len() || base[i] != a[i] { // append or change line
                diff.insert(format!("{}", i), Some(a[i].to_string()));
            }
        }

        Ok(DocMismatch{diff, mismatch_type: MismatchType::Text})
    }

    pub fn apply_txt(&self, input: &String) -> Result<String, DocParseError> {
        // assert_matches!(self.mismatch_type, MismatchType::Text);
        let mut a: Vec<&str> = input.split("\n").collect();
        let mut deletes = Vec::new();
        let mut inserts = BTreeMap::new();
        for (i, m) in &self.diff {
            let i = i.parse::<usize>().map_err(|e| DocParseError::new(e.to_string()))?;
            match m {
                None => {
                    deletes.push(i);
                }
                Some(m) => {
                    if a.len() > i {
                        a[i] = m.as_str();
                    } else {
                        inserts.insert(i, m.as_str());
                    }
                }
            }
        }
        if deletes.len() > 0 {
            deletes.sort();
            for i in 0..deletes.len() {
                let idx = deletes[deletes.len() - i-1];
                if idx < a.len() {
                    a.remove(idx);
                }
            }
        }
        if inserts.len() > 0 {
            for v in inserts.values() {
                a.push(*v);
            }
        }
        Ok(a.join("\n"))
    }
}
