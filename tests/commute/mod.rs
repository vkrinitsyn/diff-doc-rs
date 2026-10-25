//! The promise of the crate, on random changes: Base + A + B = Base + B + A when
//! patch A and patch B do not intersect - for documents, simplified texts and GNU
//! patches. Seeded, so a failure repeats; COMMUTE_SEED and COMMUTE_ROUNDS run others:
//! `COMMUTE_SEED=5 COMMUTE_ROUNDS=100000 cargo test --all-features commute`

use diff_doc::generic::{from_json, to_json, GenericValue};
use diff_doc::{MismatchDoc, MismatchDocCow, MismatchDocMut};

/// a small LCG: the same changes every run
pub struct Rng(u64);

impl Rng {
    /// `seed`, or COMMUTE_SEED's added to it
    pub fn new(seed: u64) -> Self {
        Rng(seed + std::env::var("COMMUTE_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0))
    }
    pub fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n.max(1)
    }
}

/// one random change to a JSON value: in arrays and maps, of numbers and of
/// multi-line strings
fn change(v: &mut serde_json::Value, rng: &mut Rng, tag: usize) {
    use serde_json::Value;
    match v {
        Value::Array(a) => match rng.below(6) {
            0 if !a.is_empty() => { a.remove(rng.below(a.len())); }
            1 => { let i = rng.below(a.len() + 1); a.insert(i, Value::from(1000 + tag)); }
            2 if a.len() > 1 => { let (i, j) = (rng.below(a.len()), rng.below(a.len())); a.swap(i, j); }
            3 if !a.is_empty() => { let i = rng.below(a.len()); let c = a[i].clone(); a.insert(rng.below(a.len() + 1), c); }
            _ if !a.is_empty() => { let i = rng.below(a.len()); change(&mut a[i], rng, tag); }
            _ => a.push(Value::from(tag)),
        },
        Value::Object(m) => {
            let keys: Vec<String> = m.keys().cloned().collect();
            match rng.below(4) {
                0 if !keys.is_empty() => { m.remove(&keys[rng.below(keys.len())]); }
                1 => { m.insert(format!("k{}", tag), Value::from(tag)); }
                _ if !keys.is_empty() => change(m.get_mut(&keys[rng.below(keys.len())]).unwrap(), rng, tag),
                _ => { m.insert(format!("k{}", tag), Value::from(tag)); }
            }
        }
        Value::String(s) => {
            let mut lines: Vec<String> = s.split('\n').map(String::from).collect();
            let i = rng.below(lines.len());
            match rng.below(3) {
                0 => lines[i] = format!("changed {}", tag),
                1 if lines.len() > 1 => { lines.remove(i); }
                _ => lines.insert(i, format!("new {}", tag)),
            }
            *s = lines.join("\n");
        }
        other => *other = Value::from(format!("v{}", tag)),
    }
}

const BASE: &str = r#"{"list": [1, 2, 3, 4, 5, 6, 7, 8],
  "map": {"x": 1, "y": "one\ntwo\nthree\nfour", "z": true, "n": null},
  "nested": [{"id": 1, "tags": ["a", "b"]}, {"id": 2, "tags": ["c"]}],
  "text": "first\nsecond\nthird\nfourth\nfifth"}"#;

fn doc(v: &serde_json::Value) -> GenericValue {
    from_json(&v.to_string()).unwrap()
}

/// documents: patches that do not intersect apply in either order, to the same result
#[test]
fn doc_patches_that_do_not_intersect_commute() {
    let base: serde_json::Value = serde_json::from_str(BASE).unwrap();
    let mut rng = Rng::new(7);
    let (mut apart, mut together, mut clones) = (0, 0, 0);
    for round in 0..rounds() {
        let (mut a, mut b) = (base.clone(), base.clone());
        for _ in 0..1 + rng.below(2) { change(&mut a, &mut rng, round * 2); }
        for _ in 0..1 + rng.below(2) { change(&mut b, &mut rng, round * 2 + 1); }
        let (base_doc, a_doc, b_doc) = (doc(&base), doc(&a), doc(&b));
        let pa = diff_doc::diff::Mismatch::new(&base_doc, &a_doc).unwrap();
        let pb = diff_doc::diff::Mismatch::new(&base_doc, &b_doc).unwrap();
        // each patch alone makes its document
        let mut x = base_doc.clone();
        assert!(pa.apply_mut(&mut x, true).unwrap().is_empty());
        assert_eq!(x, a_doc, "round {}: A alone\n{}", round, pa_json(&pa));
        clones += pa_json(&pa).contains("Clone") as usize;
        if pa.is_intersect(&pb).unwrap() {
            together += 1;
            continue;
        }
        apart += 1;
        let (mut ab, mut ba) = (base_doc.clone(), base_doc.clone());
        let ab_ok = pa.apply_mut(&mut ab, true).and_then(|_| pb.apply_mut(&mut ab, true));
        let ba_ok = pb.apply_mut(&mut ba, true).and_then(|_| pa.apply_mut(&mut ba, true));
        assert!(ab_ok.is_ok() && ba_ok.is_ok() && ab == ba,
            "round {}: A {} B {}\nA: {}\nB: {}\nA+B: {:?} {}\nB+A: {:?} {}", round, a, b,
            pa_json(&pa), pa_json(&pb), ab_ok, to_json(&ab).unwrap(), ba_ok, to_json(&ba).unwrap());
    }
    println!("apart {} together {} clones {}", apart, together, clones);
    assert!(apart * 10 > rounds() && together * 10 > rounds(), "apart {} together {}", apart, together);
    assert!(clones * 50 > rounds(), "Clone in {} patches only", clones);
}

/// COMMUTE_ROUNDS, or 3000
fn rounds() -> usize {
    std::env::var("COMMUTE_ROUNDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3000)
}

fn pa_json<T: serde::Serialize>(p: &T) -> String {
    serde_json::to_string(p).unwrap()
}

/// lines of text with random changes: a line changed, deleted or added
fn change_lines(lines: &mut Vec<String>, rng: &mut Rng, tag: usize) {
    let i = rng.below(lines.len());
    match rng.below(3) {
        0 => lines[i] = format!("changed {}", tag),
        1 if lines.len() > 1 => { lines.remove(i); }
        _ => lines.insert(i, format!("new {}", tag)),
    }
}

/// simplified texts, and GNU patches: the same promise
fn texts_commute<M: MismatchDoc<String> + MismatchDocCow<String> + std::fmt::Debug>(seed: u64, eol: &str) {
    let base_lines: Vec<String> = (1..=24).map(|i| format!("line {}", i)).collect();
    let base = base_lines.join("\n") + eol;
    let mut rng = Rng::new(seed);
    let (mut apart, mut together) = (0, 0);
    for round in 0..rounds() {
        let (mut a, mut b) = (base_lines.clone(), base_lines.clone());
        for _ in 0..1 + rng.below(3) { change_lines(&mut a, &mut rng, round * 2); }
        for _ in 0..1 + rng.below(3) { change_lines(&mut b, &mut rng, round * 2 + 1); }
        let (a, b) = (a.join("\n") + eol, b.join("\n") + eol);
        let pa = M::new(&base, &a).unwrap();
        let pb = M::new(&base, &b).unwrap();
        assert_eq!(pa.apply(&base).unwrap(), a, "round {}: A alone {:?}", round, pa);
        if pa.is_intersect(&pb).unwrap() {
            together += 1;
            continue;
        }
        apart += 1;
        let ab = pa.apply(&base).and_then(|x| pb.apply(&x));
        let ba = pb.apply(&base).and_then(|x| pa.apply(&x));
        assert!(matches!((&ab, &ba), (Ok(x), Ok(y)) if x == y),
            "round {}:\nA {:?}\nB {:?}\nA+B {:?}\nB+A {:?}", round, pa, pb, ab, ba);
    }
    println!("apart {} together {}", apart, together);
    assert!(apart * 10 > rounds() && together * 10 > rounds(), "apart {} together {}", apart, together);
}

#[test]
fn txt_patches_that_do_not_intersect_commute() {
    texts_commute::<diff_doc::txt::Mismatch>(11, "");
}

#[cfg(feature = "patch")]
#[test]
fn gnu_patches_that_do_not_intersect_commute() {
    texts_commute::<diff_doc::patch::Mismatch>(13, "\n");
}
