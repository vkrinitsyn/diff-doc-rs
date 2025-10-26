use std::fs::read_to_string;
use diff_doc::*;
use diff_doc::txt::Mismatch;
const BASE: &'static str = "tests/txt";

fn test_case(id: usize, a_diffs: usize, b_diffs: usize) {
    let base = read_to_string(format!("{}/case{}/base.txt", BASE, id)).unwrap();
    let a = read_to_string(format!("{}/case{}/a.txt", BASE, id)).unwrap();
    let b = read_to_string(format!("{}/case{}/b.txt", BASE, id)).unwrap();
    let result = read_to_string(format!("{}/case{}/result.txt", BASE, id)).unwrap();

    let pa = Mismatch::new(&base, &a).unwrap();
    println!("#{} A [{}]: {}", id, a_diffs, serde_json::to_string(&pa).unwrap());
    let pb = Mismatch::new(&base, &b).unwrap();
    println!("#{} B [{}]: {}", b_diffs, id, serde_json::to_string(&pb).unwrap());
    assert_eq!(pa.len(), a_diffs);
    assert_eq!(pb.len(), b_diffs);
    let x = pa.is_intersect(&pb);
    println!("#{} X [{}]: {:?}", id, a_diffs, x);
    assert_eq!(x.as_ref().err().map(|e| e.to_string()).unwrap_or("".to_string()), "".to_string());
    assert!(!x.unwrap_or(true));
    assert_eq!(pb.apply(&pa.apply(&base).unwrap()).unwrap(), result);
    assert_eq!(pa.apply(&pb.apply(&base).unwrap()).unwrap(), result);

    println!("#{} AA [{}]: {}", id, a_diffs, Mismatches::Text(pb));

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
    test_case(3, 1, 1);
}
