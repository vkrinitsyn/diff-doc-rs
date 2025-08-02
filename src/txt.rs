use std::cmp::max;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
pub(crate) use crate::{DocError};
use crate::{MismatchDoc, MismatchDocCow};

/// Simple text mismatch implementation with line number use as index.
/// The empty value on amp means remove the line.
/// To correctly use computative for patches the minimum remove index (None in value) must be greater than update
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mismatch (HashMap<usize, Option<String> >);

impl MismatchDocCow<String> for Mismatch {
    fn apply(&self, input: &String) -> Result<String, DocError> {
        let mut a: Vec<&str> = input.split("\n").collect();
        let mut deletes = Vec::new();
        let mut inserts = BTreeMap::new();
        for (i, m) in &self.0 {
            match m {
                None => {
                    deletes.push(i);
                }
                Some(m) => {
                    if a.len() > *i {
                        a[*i] = m.as_str();
                    } else {
                        inserts.insert(i, m.as_str());
                    }
                }
            }
        }

        if deletes.len() > 0 {
            deletes.sort();
            for i in 0..deletes.len() {
                let idx = *deletes[deletes.len() - i - 1];
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
impl MismatchDoc<String> for Mismatch {
    fn new(base: &String, input: &String) -> Result<Self, DocError>
    where
        Self: Sized {
        let mut diff = HashMap::new();
        let base: Vec<&str> = base.split("\n").collect();
        let a: Vec<&str> = input.split("\n").collect();

        for i in 0..max(base.len(), a.len()) {
            if i >= a.len() {
                diff.insert(i, None); // delete line
            } else if i >= base.len() || base[i] != a[i] { // append or change line
                diff.insert(i, Some(a[i].to_string()));
            }
        }

        Ok(Mismatch(diff))
    }

    fn is_intersect(&self, other: &Self) -> Result<bool, DocError> {
        let min2delete = self.min2delete();
        let min2delete_other = other.min2delete();
        let max2update = self.max2update().unwrap_or(0);
        let max2update_other = other.max2update().unwrap_or(0);

        if let Some(min) = min2delete {
            if min < max2update || min < max2update_other {
                return Err(DocError::new("the minimum remove index (None in value) must be greater than update"));
            }
        }
        if let Some(min) = min2delete_other {
            if min < max2update  || min < max2update_other {
                return Err(DocError::new("the minimum remove index (None in value) must be greater than update"));
            }
        }

        // there is no lines to delete (None value) and update after
        for (k, v) in &self.0 {
            if v.is_some() && other.0.get(k)
                .map(|o| o.is_some() && v==o).unwrap_or(false) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Mismatch {
    fn min2delete(&self) -> Option<usize> {
        self.0.iter()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| *k).min()
    }

    fn max2update(&self) -> Option<usize> {
        self.0.iter()
            .filter(|(_, v)| v.is_some())
            .map(|(k, _)| *k).max()
    }

    /// the minimum remove index (None in value) must be greater than update
    fn _valid(&self) -> bool {
        self.min2delete()
            .map(|m| m >= self.max2update().unwrap_or(0))
            .unwrap_or(true)
    }
}