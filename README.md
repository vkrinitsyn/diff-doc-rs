## diff-doc-rs

### Calculate diff on structured documents with patch file apply.

Create and restore diff mismatch from text file.

Also support standard diff for text files.

The main goal is to provide Rust lib for a 3-way offline compare-apply for implement commutative style document change:

Base vs Document_A vs Document_B should provide Base doc with mixed change from A & B if no conflicts

> Base + patch_A + patch_B = Base + patch_B + patch_A

The implementation run in steps:
1. Receive change on a Base document from host/user A and produce a Patch_A. 
2. Receive change on a Base document from host/user B and produce a Patch_B.
3. Compare for disjoint on Patch_A and Patch_B, so they can mutually apply in any order.
 -  if patches intersect, then Patch_B, which goes after Patch_A will be rejected.
4. Apply Patch_A and Patch_B on any order in any hosts to get same document.

Notice:
Array deletion must not have at index less than other patch array update, but ok for same patch.
Same apply for plain text patch for a line nimber.

### Support documents type:

- [x] Json - diff-like file format, wrapper to json_diff_ng with additional apply diff features
 todo needs to rewrite json_diff_ng as not followed objects inside array to update particular item
- [x] Txt - plain text document with default diff file format, wrapper to diffy
- [ ] XML - todo
- [ ] Yaml - todo

### TODO
- [ ] Add idempotent like capabilities for a text patching in case of 3-way. Exclude already applied hunks from a patch about to apply. 
Modify intersections to allow same changes on a particular line.
