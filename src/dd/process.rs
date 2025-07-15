use std::collections::HashMap;
use std::collections::HashSet;
use diffs::{myers, Diff, Replace};
// use diffs::{Diff, myers, Replace};
use regex::Regex;
use serde_json::Map;
use serde_json::Value;
use crate::dd::enums::DiffTreeNode;
use crate::dd::mismatch::Mismatch;
use crate::dd::Result;
use crate::dd::sort::preprocess_array;
// use crate::sort::preprocess_array;

/// Compares two string slices containing serialized json with each other, returns an error or a [`Mismatch`] structure holding all differences.
/// Internally this calls into [`compare_serde_values`] after deserializing the string slices into [`serde_json::Value`].
/// Arguments are the string slices, a bool to trigger deep sorting of arrays and ignored_keys as a list of regex to match keys against.
/// Ignoring a regex from comparison will also ignore the key from having an impact on sorting arrays.
pub fn compare_strs(
    a: &str,
    b: &str,
    sort_arrays: bool,
    ignore_keys: &[Regex],
) -> Result<Mismatch> {
    let value1 = serde_json::from_str(a)?;
    let value2 = serde_json::from_str(b)?;
    match_json(&value1, &value2, sort_arrays, ignore_keys)
}

/// Compares two [`serde_json::Value`] items with each other, returns an error or a [`Mismatch`] structure holding all differences.
/// Arguments are the values, a bool to trigger deep sorting of arrays and ignored_keys as a list of regex to match keys against.
/// Ignoring a regex from comparison will also ignore the key from having an impact on sorting arrays.
pub fn compare_json(
    a: &Value,
    b: &Value,
    ignore_keys: &[Regex],
) -> Result<Mismatch> {
    match_json(a, b, false, ignore_keys)
}

fn values_to_node(vec: Vec<(usize, &Value)>) -> DiffTreeNode {
    if vec.is_empty() {
        DiffTreeNode::Null
    } else {
        DiffTreeNode::Array(
            vec.into_iter()
                .map(|(l, v)| (l, DiffTreeNode::Value(v.clone(), v.clone())))
                .collect(),
        )
    }
}

struct ListDiffHandler<'a> {
    replaced: &'a mut Vec<(usize, usize, usize, usize)>,
    deletion: &'a mut Vec<(usize, usize)>,
    insertion: &'a mut Vec<(usize, usize)>,
}
impl<'a> ListDiffHandler<'a> {
    pub fn new(
        replaced: &'a mut Vec<(usize, usize, usize, usize)>,
        deletion: &'a mut Vec<(usize, usize)>,
        insertion: &'a mut Vec<(usize, usize)>,
    ) -> Self {
        Self {
            replaced,
            deletion,
            insertion,
        }
    }
}

impl<'a> Diff for ListDiffHandler<'a> {
    type Error = ();
    fn delete(&mut self, old: usize, len: usize, _new: usize) -> std::result::Result<(), ()> {
        self.deletion.push((old, len));
        Ok(())
    }
    fn insert(&mut self, _o: usize, new: usize, len: usize) -> std::result::Result<(), ()> {
        self.insertion.push((new, len));
        Ok(())
    }
    fn replace(
        &mut self,
        old: usize,
        len: usize,
        new: usize,
        new_len: usize,
    ) -> std::result::Result<(), ()> {
        self.replaced.push((old, len, new, new_len));
        Ok(())
    }
}

pub fn match_json(
    value1: &Value,
    value2: &Value,
    sort_arrays: bool,
    ignore_keys: &[Regex],
) -> Result<Mismatch> {
    match (value1, value2) {
        (Value::Object(a), Value::Object(b)) => process_objects(a, b, ignore_keys, sort_arrays),
        (Value::Array(a), Value::Array(b)) => process_arrays(sort_arrays, a, ignore_keys, b),
        (a, b) => process_values(a, b),
    }
}

fn process_values(a: &Value, b: &Value) -> Result<Mismatch> {
    if a == b {
        Ok(Mismatch::empty())
    } else {
        Ok(Mismatch::new(
            DiffTreeNode::Null,
            DiffTreeNode::Null,
            DiffTreeNode::Value(a.clone(), b.clone()),
        ))
    }
}

fn process_objects(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    ignore_keys: &[Regex],
    sort_arrays: bool,
) -> Result<Mismatch> {
    let diff = intersect_maps(a, b, ignore_keys);
    let mut left_only_keys = get_map_of_keys(diff.left_only, a);
    let mut right_only_keys = get_map_of_keys(diff.right_only, b);
    let intersection_keys = diff.intersection;

    let mut unequal_keys = DiffTreeNode::Null;

    for key in intersection_keys {
        let Mismatch {
            left_only: l,
            right_only: r,
            unequal_values: u,
        } = match_json(
            a.get(&key).unwrap(),
            b.get(&key).unwrap(),
            sort_arrays,
            ignore_keys,
        )?;
        left_only_keys = insert_child_key_map(left_only_keys, l, &key)?;
        right_only_keys = insert_child_key_map(right_only_keys, r, &key)?;
        unequal_keys = insert_child_key_map(unequal_keys, u, &key)?;
    }

    Ok(Mismatch::new(left_only_keys, right_only_keys, unequal_keys))
}

fn process_arrays(
    sort_arrays: bool,
    a: &Vec<Value>,
    ignore_keys: &[Regex],
    b: &Vec<Value>,
) -> Result<Mismatch> {
    let a = preprocess_array(sort_arrays, a, ignore_keys);
    let b = preprocess_array(sort_arrays, b, ignore_keys);

    let mut replaced = Vec::new();
    let mut deleted = Vec::new();
    let mut inserted = Vec::new();

    let mut diff = Replace::new(ListDiffHandler::new(
        &mut replaced,
        &mut deleted,
        &mut inserted,
    ));
    myers::diff(
        &mut diff,
        a.as_slice(),
        0,
        a.len(),
        b.as_slice(),
        0,
        b.len(),
    )
    .unwrap();

    fn extract_one_sided_values(v: Vec<(usize, usize)>, vals: &[Value]) -> Vec<(usize, &Value)> {
        v.into_iter()
            .flat_map(|(o, ol)| (o..o + ol).map(|i| (i, &vals[i])))
            .collect::<Vec<(usize, &Value)>>()
    }

    let left_only_values: Vec<_> = extract_one_sided_values(deleted, a.as_slice());
    let right_only_values: Vec<_> = extract_one_sided_values(inserted, b.as_slice());

    let mut left_only_nodes = values_to_node(left_only_values);
    let mut right_only_nodes = values_to_node(right_only_values);
    let mut diff = DiffTreeNode::Null;

    for (o, ol, n, nl) in replaced {
        let max_length = ol.max(nl);
        for i in 0..max_length {
            let inner_a = a.get(o + i).unwrap_or(&Value::Null);
            let inner_b = b.get(n + i).unwrap_or(&Value::Null);
            let cdiff = match_json(inner_a, inner_b, sort_arrays, ignore_keys)?;
            let position = o + i;
            let Mismatch {
                left_only: l,
                right_only: r,
                unequal_values: u,
            } = cdiff;
            left_only_nodes = insert_child_key_diff(left_only_nodes, l, position)?;
            right_only_nodes = insert_child_key_diff(right_only_nodes, r, position)?;
            diff = insert_child_key_diff(diff, u, position)?;
        }
    }

    Ok(Mismatch::new(left_only_nodes, right_only_nodes, diff))
}

fn get_map_of_keys(set: HashSet<String>, a: &Map<String, Value>) -> DiffTreeNode {
    if !set.is_empty() {
        DiffTreeNode::Node(
            set.iter()
                .map(|key| (String::from(key),
                            a.get(key).map(|v| DiffTreeNode::Value1(v.clone()))
                                .unwrap_or(DiffTreeNode::Null)
                            //
                ))
                .collect(),
        )
    } else {
        DiffTreeNode::Null
    }
}

fn insert_child_key_diff(
    parent: DiffTreeNode,
    child: DiffTreeNode,
    line: usize,
) -> Result<DiffTreeNode> {
    if child == DiffTreeNode::Null {
        return Ok(parent);
    }
    if let DiffTreeNode::Array(mut array) = parent {
        array.push((line, child));
        Ok(DiffTreeNode::Array(array))
    } else if let DiffTreeNode::Null = parent {
        Ok(DiffTreeNode::Array(vec![(line, child)]))
    } else {
        Err(format!("Tried to insert child: {child:?} into parent {parent:?} - structure incoherent, expected a parent array - somehow json structure seems broken").into())
    }
}

fn insert_child_key_map(
    parent: DiffTreeNode,
    child: DiffTreeNode,
    key: &String,
) -> Result<DiffTreeNode> {
    if child == DiffTreeNode::Null {
        return Ok(parent);
    }
    if let DiffTreeNode::Node(mut map) = parent {
        map.insert(String::from(key), child);
        Ok(DiffTreeNode::Node(map))
    } else if let DiffTreeNode::Null = parent {
        let mut map = HashMap::new();
        map.insert(String::from(key), child);
        Ok(DiffTreeNode::Node(map))
    } else {
        Err(format!("Tried to insert child: {child:?} into parent {parent:?} - structure incoherent, expected a parent object - somehow json structure seems broken").into())
    }
}

struct MapDifference {
    left_only: HashSet<String>,
    right_only: HashSet<String>,
    intersection: HashSet<String>,
}

impl MapDifference {
    pub fn new(
        left_only: HashSet<String>,
        right_only: HashSet<String>,
        intersection: HashSet<String>,
    ) -> Self {
        Self {
            right_only,
            left_only,
            intersection,
        }
    }
}

fn intersect_maps(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    ignore_keys: &[Regex],
) -> MapDifference {
    let mut intersection = HashSet::new();
    let mut left = HashSet::new();

    let mut right = HashSet::new();
    for a_key in a
        .keys()
        .filter(|k| ignore_keys.iter().all(|r| !r.is_match(k.as_str())))
    {
        if b.contains_key(a_key) {
            intersection.insert(String::from(a_key));
        } else {
            left.insert(String::from(a_key));
        }
    }
    for b_key in b
        .keys()
        .filter(|k| ignore_keys.iter().all(|r| !r.is_match(k.as_str())))
    {
        if !a.contains_key(b_key) {
            right.insert(String::from(b_key));
        }
    }

    MapDifference::new(left, right, intersection)
}

#[cfg(test)]
mod tests {
    use maplit::hashmap;
    use serde_json::json;
    use super::*;

    #[test]
    fn sorting_ignores_ignored_keys() {
        let data1: Value =
            serde_json::from_str(r#"[{"a": 1, "b":2 }, { "a": 2, "b" : 1 }]"#).unwrap();
        let ignore = [Regex::new("a").unwrap()];
        let sorted_ignores = preprocess_array(true, data1.as_array().unwrap(), &ignore);
        let sorted_no_ignores = preprocess_array(true, data1.as_array().unwrap(), &[]);

        assert_eq!(
            sorted_ignores
                .first()
                .unwrap()
                .as_object()
                .unwrap()
                .get("b")
                .unwrap()
                .as_i64()
                .unwrap(),
            1
        );
        assert_eq!(
            sorted_no_ignores
                .first()
                .unwrap()
                .as_object()
                .unwrap()
                .get("b")
                .unwrap()
                .as_i64()
                .unwrap(),
            2
        );
    }

    #[test]
    fn test_arrays_sorted_objects_ignored() {
        let data1 = r#"[{"c": {"d": "e"} },"b","c"]"#;
        let data2 = r#"["b","c",{"c": {"d": "f"} }]"#;
        let ignore = Regex::new("d").unwrap();
        let diff = compare_strs(data1, data2, true, &[ignore]).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_arrays_sorted_simple() {
        let data1 = r#"["a","b","c"]"#;
        let data2 = r#"["b","c","a"]"#;
        let diff = compare_strs(data1, data2, true, &[]).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_arrays_sorted_objects() {
        let data1 = r#"[{"c": {"d": "e"} },"b","c"]"#;
        let data2 = r#"["b","c",{"c": {"d": "e"} }]"#;
        let diff = compare_strs(data1, data2, true, &[]).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_arrays_deep_sorted_objects() {
        let data1 = r#"[{"c": ["d","e"] },"b","c"]"#;
        let data2 = r#"["b","c",{"c": ["e", "d"] }]"#;
        let diff = compare_strs(data1, data2, true, &[]).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_arrays_deep_sorted_objects_with_arrays() {
        let data1 = r#"[{"a": [{"b": ["3", "1"]}] }, {"a": [{"b": ["2", "3"]}] }]"#;
        let data2 = r#"[{"a": [{"b": ["2", "3"]}] }, {"a": [{"b": ["1", "3"]}] }]"#;
        let diff = compare_strs(data1, data2, true, &[]).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_arrays_deep_sorted_objects_with_outer_diff() {
        let data1 = r#"[{"c": ["d","e"] },"b"]"#;
        let data2 = r#"["b","c",{"c": ["e", "d"] }]"#;
        let diff = compare_strs(data1, data2, true, &[]).unwrap();
        assert!(!diff.is_empty());
        let insertions = diff.right_only.get_diffs();
        assert_eq!(insertions.len(), 1);
        assert_eq!(insertions.first().unwrap().to_string(), r#".[2]"#);
    }

    #[test]
    fn test_arrays_deep_sorted_objects_with_inner_diff() {
        let data1 = r#"["a",{"c": ["d","e", "f"] },"b"]"#;
        let data2 = r#"["b",{"c": ["e","d"] },"a"]"#;
        let diff = compare_strs(data1, data2, true, &[]).unwrap();
        assert!(!diff.is_empty());
        let deletions = diff.left_only.get_diffs();

        assert_eq!(deletions.len(), 1);
        assert_eq!(
            deletions.first().unwrap().to_string(),
            r#".[0].c.[2]"#
        );
    }

    #[test]
    fn test_arrays_deep_sorted_objects_with_inner_diff_mutation() {
        let data1 = r#"["a",{"c": ["d", "f"] },"b"]"#;
        let data2 = r#"["b",{"c": ["e","d"] },"a"]"#;
        let diffs = compare_strs(data1, data2, true, &[]).unwrap();
        assert!(!diffs.is_empty());
        let diffs = diffs.unequal_values.get_diffs();

        assert_eq!(diffs.len(), 1);
        assert_eq!(
            diffs.first().unwrap().to_string(),
            r#".[0].c.[1]"#
        );
    }

    #[test]
    fn test_arrays_simple_diff() {
        let data1 = r#"["a","b","c"]"#;
        let data2 = r#"["a","b","d"]"#;
        let diff = compare_strs(data1, data2, false, &[]).unwrap();
        assert_eq!(diff.left_only, DiffTreeNode::Null);
        assert_eq!(diff.right_only, DiffTreeNode::Null);
        let diff = diff.unequal_values.get_diffs();
        assert_eq!(diff.len(), 1);
        assert_eq!(diff.first().unwrap().to_string(), r#".[2]"#); // .("c" != "d")"#);
    }

    #[test]
    fn test_arrays_more_complex_diff() {
        let data1 = r#"["a","b","c"]"#;
        let data2 = r#"["a","a","b","d"]"#;
        let diff = compare_strs(data1, data2, false, &[]).unwrap();

        let changes_diff = diff.unequal_values.get_diffs();
        assert_eq!(diff.left_only, DiffTreeNode::Null);

        assert_eq!(changes_diff.len(), 1);
        assert_eq!(
            changes_diff.first().unwrap().to_string(),
            r#".[2]"#
        );
        let insertions = diff.right_only.get_diffs();
        assert_eq!(insertions.len(), 1);
        assert_eq!(insertions.first().unwrap().to_string(), r#".[0]"#);
    }

    #[test]
    fn test_arrays_extra_left() {
        let data1 = r#"["a","b","c"]"#;
        let data2 = r#"["a","b"]"#;
        let diff = compare_strs(data1, data2, false, &[]).unwrap();

        let diffs = diff.left_only.get_diffs();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs.first().unwrap().to_string(), r#".[2]"#);
        assert_eq!(diff.unequal_values, DiffTreeNode::Null);
        assert_eq!(diff.right_only, DiffTreeNode::Null);
    }

    #[test]
    fn test_arrays_extra_right() {
        let data1 = r#"["a","b"]"#;
        let data2 = r#"["a","b","c"]"#;
        let diff = compare_strs(data1, data2, false, &[]).unwrap();

        let diffs = diff.right_only.get_diffs();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs.first().unwrap().to_string(), r#".[2]"#);
        assert_eq!(diff.unequal_values, DiffTreeNode::Null);
        assert_eq!(diff.left_only, DiffTreeNode::Null);
    }

    #[test]
    fn test_json_extra_right_left() {
        let data1 = r#"{"a":"_", "b":{"a":"a"}}"#;
        let data2 = r#"{"c":"_", "b":{"a":"b"}}"#;
        let diff = compare_strs(data1, data2, false, &[]).unwrap();

        // assert_eq!(diff.unequal_values, DiffTreeNode::Null);
        let diffs = diff.unequal_values.get_diffs();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path.len(), 2);
        assert_eq!(diffs[0].path[1].to_string(), "a");
        assert!(diffs[0].old_value.is_some());
        assert!(diffs[0].value.is_some());
        assert_eq!(diffs[0].old_value.unwrap(), "a");
        assert_eq!(diffs[0].value.unwrap(), "b");

        let l_diffs = diff.left_only.get_diffs();
        assert_eq!(l_diffs.len(), 1);
        assert_eq!(l_diffs[0].path[0].to_string(), "a");
        assert!(l_diffs[0].value.is_some());
        // assert!(l_diffs[0].new_value.is_none());
        assert_eq!(l_diffs[0].value.unwrap().to_string(), "\"_\"");

        let r_diffs = diff.right_only.get_diffs();
        assert_eq!(r_diffs.len(), 1);
        assert_eq!(r_diffs[0].path[0].to_string(), "c");
        assert!(r_diffs[0].old_value.is_none());
        assert!(r_diffs[0].value.is_some());
        assert_eq!(r_diffs[0].value.unwrap().to_string(), "\"_\"");

    }

    #[test]
    fn long_insertion_modification() {
        let data1 = r#"["a","b","a"]"#;
        let data2 = r#"["a","c","c","c","a"]"#;
        let diff = compare_strs(data1, data2, false, &[]).unwrap();
        let diffs = diff.unequal_values.get_diffs();

        assert_eq!(diffs.len(), 3);
        let diffs: Vec<_> = diffs.into_iter().map(|d| d.to_string()).collect();

        // assert!(diffs.contains(&r#".[3].(null != "c")"#.to_string()));
        assert!(diffs.contains(&r#".[3]"#.to_string()));
        // assert!(diffs.contains(&r#".[1].("b" != "c")"#.to_string()));
        assert!(diffs.contains(&r#".[1]"#.to_string()));
        // assert!(diffs.contains(&r#".[2].("a" != "c")"#.to_string()));
        assert!(diffs.contains(&r#".[2]"#.to_string()));
        assert_eq!(diff.right_only, DiffTreeNode::Null);
        assert_eq!(diff.left_only, DiffTreeNode::Null);
    }

    #[test]
    fn test_arrays_object_extra() {
        let data1 = r#"["a","b"]"#;
        let data2 = r#"["a","b", {"c": {"d": "e"} }]"#;
        let diff = compare_strs(data1, data2, false, &[]).unwrap();

        let diffs = diff.right_only.get_diffs();
        assert_eq!(diffs.len(), 1);
        assert_eq!(
            diffs.first().unwrap().to_string(),
            r#".[2]"# // .({"c":{"d":"e"}})"#
        );
        assert_eq!(diff.unequal_values, DiffTreeNode::Null);
        assert_eq!(diff.left_only, DiffTreeNode::Null);
    }


    #[test]
    fn nested_diff_left() {
        let data1 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":5,
                    "f":9,
                    "h":{
                        "i":true,
                        "j":false
                    }
                }
            }
        }"#;
        let data2 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":5
                }
            }
        }"#;
            
        let expected_left = DiffTreeNode::Node(hashmap! {
        "b".to_string() => DiffTreeNode::Node(hashmap! {
                "c".to_string() => DiffTreeNode::Node(hashmap! {
                        "f".to_string() => DiffTreeNode::Value1(Value::from(9)),
                        "h".to_string() => DiffTreeNode::Value1( json!({ "i":true,  "j":false } ))
                }),
            }),
        });
        let expected = Mismatch::new(expected_left, DiffTreeNode::Null, DiffTreeNode::Null);

        let mismatch = compare_strs(data1, data2, true, &[]).unwrap();
        assert_eq!(mismatch, expected, "Diff was incorrect.");
    }


    #[test]
    fn nested_diff_right() {
        let data1 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":5
                }
            }
        }"#;
        let data2 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":5,
                    "f":9,
                    "h":{
                        "i":true,
                        "j":false
                    }
                }
            }
        }"#;
            
        let expected = DiffTreeNode::Node(hashmap! {
        "b".to_string() => DiffTreeNode::Node(hashmap! {
                "c".to_string() => DiffTreeNode::Node(hashmap! {
                        "f".to_string() => DiffTreeNode::Value1(Value::from(9)),
                        "h".to_string() => DiffTreeNode::Value1( json!({ "i":true, "j":false } )) 
                }),
            }),
        });
        let expected = Mismatch::new(DiffTreeNode::Null, expected, DiffTreeNode::Null);

        let mismatch = compare_strs(data1, data2, true, &[]).unwrap();
        assert_eq!(mismatch, expected, "Diff was incorrect.");
    }

    #[test]
    fn nested_diff_u() {
        let data1 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":6,
                    "f":8,
                    "h":{
                        "i":"-",
                        "j":"_"
                    }
                }
            }
        }"#;
        let data2 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":false,
                    "e":6,
                    "f":9,
                    "h":{
                        "i":"_",
                        "j":"_"
                    }
                }
            }
        }"#;

        let expected = DiffTreeNode::Node(hashmap! {
        "b".to_string() => DiffTreeNode::Node(hashmap! {
                "c".to_string() => DiffTreeNode::Node(hashmap! {
                    "d".to_string() => DiffTreeNode::Value(Value::from(true),Value::from(false)),
                    "f".to_string() => DiffTreeNode::Value(Value::from(8), Value::from(9)),
                    "h".to_string() => DiffTreeNode::Node( hashmap! {
                        "i".to_string() => DiffTreeNode::Value(Value::from("-"), Value::from("_")),
                    })
                }),
            }),
        });
        let expected = Mismatch::new(DiffTreeNode::Null, DiffTreeNode::Null, expected);

        let mismatch = compare_strs(data1, data2, true, &[]).unwrap();
        assert_eq!(mismatch, expected, "Diff was incorrect.");
    }

    #[test]
    fn no_diff() {
        let data1 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":5,
                    "f":9,
                    "h":{
                        "i":true,
                        "j":false
                    }
                }
            }
        }"#;
        let data2 = r#"{
            "a":"b",
            "b":{
                "c":{
                    "d":true,
                    "e":5,
                    "f":9,
                    "h":{
                        "i":true,
                        "j":false
                    }
                }
            }
        }"#;

        assert_eq!(
            compare_strs(data1, data2, false, &[]).unwrap(),
            Mismatch::new(DiffTreeNode::Null, DiffTreeNode::Null, DiffTreeNode::Null)
        );
    }

    #[test]
    fn no_json() {
        let data1 = r#"{}"#;
        let data2 = r#"{}"#;

        assert_eq!(
            compare_strs(data1, data2, false, &[]).unwrap(),
            Mismatch::empty()
        );
    }

    #[test]
    fn parse_err_source_one() {
        let invalid_json1 = r#"{invalid: json}"#;
        let valid_json2 = r#"{"a":"b"}"#;
        compare_strs(invalid_json1, valid_json2, false, &[])
            .expect_err("Parsing invalid JSON didn't throw an error");
    }

    #[test]
    fn parse_err_source_two() {
        let valid_json1 = r#"{"a":"b"}"#;
        let invalid_json2 = r#"{invalid: json}"#;
        compare_strs(valid_json1, invalid_json2, false, &[])
            .expect_err("Parsing invalid JSON didn't throw an err");
    }
}
