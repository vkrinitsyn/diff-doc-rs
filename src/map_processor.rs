use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::{DefaultHasher, Hash, Hasher};
use serde::{Deserialize, Deserializer, Serialize};
use crate::{DocError, MismatchDoc, MismatchDocMut};
use crate::generic::*;

/// identify min_map_changes
pub(crate) fn min_map_changes(base_map: &HashMap<String, GenericValue>, input_map: &HashMap<String, GenericValue>, path: &Vec<DocIndex>) -> Vec<Hunk> {
        let mut diff = Vec::new();
        let mut input: HashSet<&String>  = input_map.keys().collect();

        for (base_key, base_value) in base_map {
            input.remove(base_key);
            match input_map.get(base_key) {
                None => {
                    Hunk::append(&mut diff, path, DocIndex::Name(base_key.clone()), HunkAction::Remove);
                }
                Some(b) => {
                    let mut p = path.clone();
                    p.push(DocIndex::Name(base_key.clone()));
                    let mut v = GenericValue::diff(base_value, b, &p);
                    diff.append(&mut v);
                }
            }
        }

        for key in input {
            if let Some(value) = input_map.get(key) {
                Hunk::append(&mut diff, path, DocIndex::Name(key.clone()), HunkAction::Update(value.clone()));
            }
        }

        diff
    }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_test_1() {}
}