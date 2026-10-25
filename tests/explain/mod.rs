//! Where patches intersect, and why: the intersecting cases of tests/json, tests/txt and
//! tests/patch_diff, each with the change of A and of B that do not commute

use std::fs::read_to_string;
use diff_doc::{Intersection, MismatchDoc, Reason};
use diff_doc::generic::from_json;

fn json(id: usize) -> Vec<Intersection> {
    let read = |name: &str| from_json(&read_to_string(format!("tests/json/case{}/{}.json", id, name)).unwrap()).unwrap();
    let base = read("base");
    let pa = diff_doc::diff::Mismatch::new(&base, &read("a")).unwrap();
    let pb = diff_doc::diff::Mismatch::new(&base, &read("b")).unwrap();
    pa.intersections(&pb).unwrap()
}

fn text<M: MismatchDoc<String>>(dir: &str, id: usize) -> Vec<Intersection> {
    let read = |name: &str| read_to_string(format!("tests/{}/case{}/{}.txt", dir, id, name)).unwrap();
    let base = read("base");
    M::new(&base, &read("a")).unwrap().intersections(&M::new(&base, &read("b")).unwrap()).unwrap()
}

/// the one intersection of a case: its reason, and where
fn one(found: Vec<Intersection>, reason: Reason, at: &str) {
    for i in &found {
        println!("{}", i);
    }
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!((found[0].reason, found[0].at.as_str()), (reason, at), "{:?}", found);
    assert!(found[0].to_string().starts_with("DD-I"), "{}", found[0]);
}

#[test]
fn documents() {
    one(json(8), Reason::Inside, "/map");
    one(json(9), Reason::OtherElement, "/list/2");
    one(json(10), Reason::Inside, "/nested/1");
    one(json(11), Reason::ArrayShift, "/list");
    one(json(12), Reason::SameNode, "/version");
}

#[test]
fn texts() {
    one(text::<diff_doc::txt::Mismatch>("txt", 7), Reason::SameLine, "5");
    one(text::<diff_doc::txt::Mismatch>("txt", 8), Reason::LineMoved, "2");
    one(text::<diff_doc::txt::Mismatch>("txt", 9), Reason::BothMoveLines, "3");
}

#[cfg(feature = "patch")]
#[test]
fn gnu_patches() {
    one(text::<diff_doc::patch::Mismatch>("patch_diff", 4), Reason::HunksOverlap, "12-14");
}

/// disjoint: nothing to explain
#[test]
fn disjoint_has_none() {
    for id in 4..=7 {
        assert!(json(id).is_empty(), "json {}: {:?}", id, json(id));
    }
    for id in 4..=6 {
        assert!(text::<diff_doc::txt::Mismatch>("txt", id).is_empty(), "txt {}", id);
    }
}
