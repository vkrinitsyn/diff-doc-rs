extern crate diff_doc;

use std::fs::read_to_string;
use diff_doc::{MismatchDoc, MismatchDocMut};
use diff_doc::diff::Mismatch;
use diff_doc::generic::{from_json, to_yaml, GenericValue};

fn read_json(id: usize, name: &str) -> GenericValue {
    from_json(read_to_string(format!("tests/json/case{}/{}.json", id, name)).unwrap().as_str()).unwrap()
}

fn test_case(id: usize, a_diffs: usize, b_diffs: usize) {
    let base = read_json(id, "base");
    let a = read_json(id, "a");
    let b = read_json(id, "b");
    let result = read_json(id, "result");

    let pa = Mismatch::new(&base, &a).unwrap();
    println!("#{} A [{}]: {:?}", id, a_diffs, pa);
    let pb = Mismatch::new(&base, &b).unwrap();
    println!("#{} B [{}]: {:?}", b_diffs, id, pb);
    assert_eq!(pa.len(), a_diffs);
    assert_eq!(pb.len(), b_diffs);
    let x = pa.is_intersect(&pb);
    assert!(x.is_ok(), "is_intersect parsing");
    assert!(!x.unwrap(), "is_intersect");

    let mut base_a = base.clone();
    let mut base_b = base.clone();

    // Base + A + B:
    let _ = pa.apply_mut(&mut base_a, false).unwrap();
    let _ = pb.apply_mut(&mut base_a, false).unwrap();
    // Base + B + A:
    let _ = pb.apply_mut(&mut base_b, false).unwrap();
    let _ = pa.apply_mut(&mut base_b, false).unwrap();

    assert_eq!(base_a, base_b, "{} <>\n{}", to_yaml(&base_a).unwrap(), to_yaml(&base_b).unwrap());
    assert_eq!(base_a, result, "{} <>\n{}<>\n{}", to_yaml(&base_a).unwrap(), to_yaml(&base_b).unwrap(), to_yaml(&result).unwrap());
    assert_eq!(base_b, result, "{} <>\n{}<>\n{}", to_yaml(&base_a).unwrap(), to_yaml(&base_b).unwrap(), to_yaml(&result).unwrap());
}

#[test] /// basic object update
fn test_case1() {
    test_case(1, 1, 1);
}

#[test] /// basic array update
fn test_case2() {
    test_case(2, 1, 1);
}

#[test] /// mixed object array update
fn test_case3() {
    test_case(3, 1, 2);
}



