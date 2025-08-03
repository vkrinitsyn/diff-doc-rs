#![allow(warnings, unused)]
extern crate diff_doc;

use std::cmp::{max, min};
use std::collections::HashSet;
use diffy::{HunkRange, Patch};
use diff_doc::*;

#[test]
fn test_txt_diff_0() {
    use diffy::create_patch;
    let original = "1\n2\n3\n4\nThe Way of Kings\nWords of Radiance\n0\n1\n2\n3\n4\n5\n";
    let modified = "1\n2\n3\n4\nThe Way of Kings\nWords of Radiance\nline3 add: Oathbringer\n0\n1\n2\n3\n4\n5\n";

    let patch = create_patch(original, modified);
    println!("{}", patch);
    ranges(&patch);

}

#[test]
fn test_txt_diff_01() {
    use diffy::create_patch;
    let original = "1\n2\n3\nThe Way of Kings\nWords of Radiance\n0\n1\n2\n3\n4\n5\n";
    let modified = "1\n2\n3\nThe Way of Kings\nWords of Radiance\nline3 add: Oathbringer\n0\n1\n2\n3\n4\n5\n";

    let patch = create_patch(original, modified);
    println!("{}", patch);
    ranges(&patch);
}

#[test]
fn test_txt_diff_1() {
    use diffy::create_patch;
    let original = "1\n2\n3\n4\n5\nThe Way of Kings\nWords of Radiance\n00\n1\n2\n3\n4\n5\n\neof\n";
    let modified = "1\n2\n3\n4\n5\nThe Way of Kings\nWords of Radiance - line2 change\n00\n1\n2\n3\n4\n5\n\neof\n";

    let patch = create_patch(original, modified);
    println!("{}", patch);
    ranges(&patch);
}
#[test]
fn test_txt_diff__1() {
    use diffy::create_patch;
    let original = "1\n2\n3\n4\n5\nThe Way of Kings\nWords of Radiance\n00\n1\n2\n3\n4\n5\n\neof\n";
    let modified = "1\n2\n3\n4\n5\nThe Way of Kings\n000\n1\n2\n3\n4\n5\n\neof\n";

    let patch = create_patch(original, modified);
    // patch.to_string()
    println!("{}", patch);
    println!("");
    ranges(&patch);
    // let patch = Patch::from_str(s).unwrap();
}



fn irange(h: &HunkRange, diff: &mut HashSet<usize>) -> usize {
    let range = max(h.len(), 7);
    for i in h.start()+range / 2 .. h.end()+1-range / 2 {
        diff.insert(i);
    }
    h.len()
}

/// Text intersect calculation of two Patches:
/// - not intersect if even and not same lines
/// - is intersected if adding / removing lines (marked uneven) less than other
///
/// Returns: 
/// - minimum number of line when adding / removing lines
/// - set of changing lines
fn ranges(patch: &Patch<str>) -> (Option<usize>, HashSet<usize>) {
    let mut diff = HashSet::new();
    let mut u = None;
    for h in patch.hunks() {
        let old = irange(&h.old_range(), &mut diff);
        let new = irange(&h.new_range(), &mut diff);
        if old !=new {
            u = Some(min(old, new));
            println!(">uneven<");
        }
    }

    println!("{:?}\n\n", diff);
    (u, diff)

}