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




/// case `id`: with a result, A and B do not intersect, and Base + A + B = Base + B + A =
/// result; without one, they intersect
fn check(id: usize) {
    let base = read_json(id, "base");
    let pa = Mismatch::new(&base, &read_json(id, "a")).unwrap();
    let pb = Mismatch::new(&base, &read_json(id, "b")).unwrap();
    println!("#{} A: {:?}\n#{} B: {:?}", id, pa, id, pb);
    let x = (pa.is_intersect(&pb).unwrap(), pb.is_intersect(&pa).unwrap());
    if !std::path::Path::new(&format!("tests/json/case{}/result.json", id)).exists() {
        assert_eq!(x, (true, true), "#{} intersect\nA: {:?}\nB: {:?}", id, pa, pb);
        return;
    }
    assert_eq!(x, (false, false), "#{} intersect\nA: {:?}\nB: {:?}", id, pa, pb);
    let result = read_json(id, "result");
    for (first, second) in [(&pa, &pb), (&pb, &pa)] {
        let mut doc = base.clone();
        assert!(first.apply_mut(&mut doc, true).unwrap().is_empty());
        assert!(second.apply_mut(&mut doc, true).unwrap().is_empty());
        assert_eq!(doc, result, "#{}\n{}", id, to_yaml(&doc).unwrap());
    }
}

#[test] /// map keys added and removed, values of other types
fn test_case4() { check(4); }

#[test] /// multi-line strings, a line of each changed
fn test_case5() { check(5); }

#[test] /// array elements swapped, a map changed
fn test_case6() { check(6); }

#[test] /// an array element copied in before the last: a Clone, another key changed
fn test_case7() { check(7); }

#[test] /// intersect: a node removed, changed inside by the other
fn test_case8() { check(8); }

#[test] /// intersect: a swap's other element updated
fn test_case9() { check(9); }

#[test] /// intersect: elements swapped, one changed inside by the other
fn test_case10() { check(10); }

#[test] /// intersect: a swap, an element removed between the swapped
fn test_case11() { check(11); }

#[test] /// intersect: one value, two updates
fn test_case12() { check(12); }
