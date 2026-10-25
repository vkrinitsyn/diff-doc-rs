//! GNU patches (diffy): Base + A + B = Base + B + A = result when they do not intersect

use std::fs::read_to_string;
use diff_doc::{MismatchDoc, MismatchDocCow};
use diff_doc::patch::Mismatch;

/// case `id`: with a result, A and B do not intersect, and Base + A + B = Base + B + A =
/// result; without one, they intersect
fn check(id: usize) {
    let read = |name: &str| read_to_string(format!("tests/patch_diff/case{}/{}.txt", id, name));
    let base = read("base").unwrap();
    let pa = Mismatch::new(&base, &read("a").unwrap()).unwrap();
    let pb = Mismatch::new(&base, &read("b").unwrap()).unwrap();
    println!("#{} A: {:?}\n#{} B: {:?}", id, pa, id, pb);
    let x = (pa.is_intersect(&pb).unwrap(), pb.is_intersect(&pa).unwrap());
    match read("result") {
        Err(_) => assert_eq!(x, (true, true), "#{} intersect\nA: {:?}\nB: {:?}", id, pa, pb),
        Ok(result) => {
            assert_eq!(x, (false, false), "#{} intersect\nA: {:?}\nB: {:?}", id, pa, pb);
            assert_eq!(pb.apply(&pa.apply(&base).unwrap()).unwrap(), result, "#{} A+B", id);
            assert_eq!(pa.apply(&pb.apply(&base).unwrap()).unwrap(), result, "#{} B+A", id);
        }
    }
}

#[test] /// changes apart, beyond their context
fn test_case1() { check(1); }

#[test] /// the same change in both, applied once
fn test_case2() { check(2); }

#[test] /// lines added at the top, a line at the bottom changed
fn test_case3() { check(3); }

#[test] /// intersect: changes within each other's context
fn test_case4() { check(4); }
