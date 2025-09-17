extern crate diff_doc;

use std::fs::read_to_string;
use serde_json::Value;
use diff_doc::json::Mismatch;
use diff_doc::{MismatchDoc, MismatchDocMut};

fn read_json(id: usize, name: &str) -> Value {
    serde_json::from_str(read_to_string(format!("tests/json/case{}/{}.json", id, name)).unwrap().as_str()).unwrap()
}

fn test_case(id: usize, a_cnt: usize, b_cnt: usize) {
    let base = read_json(id, "base");
    let a = read_json(id, "a");
    let b = read_json(id, "b");
    let result = read_json(id, "result");

    let pa = Mismatch::new(&base, &a).unwrap();
    println!("#{} A [{}]: {}", id, a_cnt, pa);
    let pb = Mismatch::new(&base, &b).unwrap();
    println!("#{} B [{}]: {}", b_cnt, id, pb);
    assert_eq!(pa.len(), a_cnt);
    assert_eq!(pb.len(), b_cnt);
    let x = pa.is_intersect(&pb);
    assert!(x.is_ok(), "is_intersect parsing");
    assert!(!x.unwrap(), "is_intersect");

    let mut base_a = base.clone();
    let mut base_b = base.clone();

    // Base + A + B:
    let _ = pa.apply_mut(&mut base_a).unwrap();
    let _ = pb.apply_mut(&mut base_a).unwrap();
    // Base + B + A:
    let _ = pb.apply_mut(&mut base_b).unwrap();
    let _ = pa.apply_mut(&mut base_b).unwrap();

    assert_eq!(base_a, base_b);
    assert_eq!(base_a, result);
    assert_eq!(base_b, result);
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
    test_case(3, 2, 1);
}



