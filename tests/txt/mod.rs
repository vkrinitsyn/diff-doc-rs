use std::fs::read_to_string;
use serde_json::json;
use diff_doc::{MismatchDoc, Mismatches};
use diff_doc::txt::Mismatch;
const BASE: &'static str = "tests/txt";

fn test_case(id: usize, a_cnt: usize, b_cnt: usize) {
    let base = read_to_string(format!("{}/case{}/base.txt", BASE, id)).unwrap();
    let a = read_to_string(format!("{}/case{}/a.txt", BASE, id)).unwrap();
    let b = read_to_string(format!("{}/case{}/b.txt", BASE, id)).unwrap();
    let result = read_to_string(format!("{}/case{}/result.txt", BASE, id)).unwrap();

    let pa = Mismatch::new(&base, &a).unwrap();
    println!("#{} A [{}]: {}", id, a_cnt, serde_json::to_string(&pa).unwrap());
    let pb = Mismatch::new(&base, &b).unwrap();
    println!("#{} B [{}]: {}", b_cnt, id, serde_json::to_string(&pb).unwrap());
    // assert_eq!(pa.diff.len(), a_cnt);
    // assert_eq!(pb.diff.len(), b_cnt);
    let x = pa.is_intersect(&pb);
    assert_eq!(x.as_ref().err().map(|e| e.to_string()).unwrap_or("".to_string()), "".to_string());
    assert!(!x.unwrap_or(true));
    assert_eq!(pb.apply_to(&pa.apply_to(&base).unwrap()).unwrap(), result);
    assert_eq!(pa.apply_to(&pb.apply_to(&base).unwrap()).unwrap(), result);

    println!("#{} AA [{}]: {}", id, a_cnt, Mismatches::Text(pb));

}

#[test]
fn test_case1() {
    test_case(1, 1, 1);
}

#[test]
fn test_case2() {
    test_case(2, 1, 2);
}

#[test]
fn test_case3() {
    test_case(3, 2, 3);
}
