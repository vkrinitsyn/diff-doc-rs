## diff-doc-rs

### Calculate diff on structured documents with commutative patch apply.

Create and restore diff mismatch from text or json file.

> [!NOTE]
> Providing patch with minimal changes to list and map structure.
> In addition to the regular patch operations of insert, update, and delete, it includes swap and clone to reduce patch size.

Also support standard diff for text files.

The main goal is to provide Rust lib for a 3-way offline compare-apply for implement commutative style document change:

Base vs Document_A vs Document_B should provide Base doc with mixed change from A & B if no conflicts present.

> [!IMPORTANT]
> Base + patch_A + patch_B = Base + patch_B + patch_A; if patch_A and patch_B are disjoint.

The implementation run in steps:
1. Receive change on a Base document from host/user A and produce a Patch_A. 
2. Receive change on a Base document from host/user B and produce a Patch_B.
3. Compare for disjoint on Patch_A and Patch_B, so they can mutually apply in any order.
 -  if patches intersect, then Patch_B, which goes after Patch_A will be rejected.
4. Apply Patch_A and Patch_B in **ANY** order in any hosts to get same document.

Notice:
Array deletion must not have at index less than other patch array update, neither two different index's delete.
Same apply for simplified plain text patch where line nimber use as index.

### Support documents type:

- [x] Json - default format for Postgres document storage
- [x] Text - plain text simplified diff as arrays of strings
- [x] XML - serde-xml-rs
- [x] Yaml - serde_yaml
- [x] Toml - toml-rs
- [x] Diff - plain text document with default diff file format, wrapper to diffy (optional feature)

### TODO
- Add more examples to integration tests
