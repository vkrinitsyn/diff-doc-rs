## diff-doc-rs

[![ci](https://github.com/vkrinitsyn/diff-doc-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/vkrinitsyn/diff-doc-rs/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/vkrinitsyn/diff-doc-rs/graph/badge.svg)](https://codecov.io/gh/vkrinitsyn/diff-doc-rs)
[![audit](https://github.com/vkrinitsyn/diff-doc-rs/actions/workflows/audit.yml/badge.svg)](https://github.com/vkrinitsyn/diff-doc-rs/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/diff_doc.svg)](https://crates.io/crates/diff_doc)
[![docs.rs](https://docs.rs/diff_doc/badge.svg)](https://docs.rs/diff_doc)
[![license](https://img.shields.io/crates/l/diff_doc.svg)](LICENSE)

### Calculate diff on structured documents with commutative patch apply.

Provided by [DBinvent](https://www.dbinvent.com), and an essential part of
[YT Cluster](https://www.dbinvent.com/cluster/): each node's schema gate compares
its live schema with the leader's through these diffs before any data is compared,
and when two writers change the same list in a replicated jsonb document, both
changes apply if they are disjoint.

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

### Where patches intersect

`is_intersect` says whether two patches commute; `intersections` says where they do not:
each pair of a change of A and one of B that would apply differently in the other
order, why, and where - a document's path as a JSON pointer, a text's line, a GNU
patch's lines of the base. Empty when the patches are disjoint.

```rust
use diff_doc::{MismatchDoc, diff::Mismatch, generic::from_json};

let base = from_json(r#"{"map": {"x": 1, "n": null}, "list": [1, 2, 3]}"#)?;
let a = Mismatch::new(&base, &from_json(r#"{"list": [1, 2, 3]}"#)?)?;
let b = Mismatch::new(&base, &from_json(r#"{"map": {"x": 1}, "list": [1, 2, 3]}"#)?)?;
for i in a.intersections(&b)? {
    println!("{i}");
    // DD-I001 change 1 of A and change 1 of B: one changes the node /map, the other inside it
}
```

The reasons: a node changed and inside it (`Inside`), a swap or clone reading the
other's element (`OtherElement`), one node changed differently (`SameNode`), an array
element moved by the other's remove or insert (`ArrayShift`), a node taken as a map and
as an array (`NodeKind`); in a text, both moving lines (`BothMoveLines`), a line moved
by the other (`LineMoved`), one line changed differently (`SameLine`); in GNU patches,
hunks reading the same lines (`HunksOverlap`).

### Support documents type:

- [x] Json - default format for Postgres document storage
- [x] Text - plain text simplified diff as arrays of strings
- [x] XML - roxmltree, mapped to the generic document by this crate: attributes as `@name`,
  a repeated element as an array, text beside elements as `$text` (see `src/xml.rs`)
- [x] Yaml - serde-saphyr
- [x] Toml - toml-rs
- [x] Diff - plain text document with default diff file format, wrapper to diffy (optional feature)

### Features

- `i18n` (default) - the error messages in en, ru, fr, de, zh and ja, chosen by
  `diff_doc::set_lang` or `LANG`; without it English only, and no rust-i18n. Each of
  diff_doc's own errors starts with its code, `DD-Ennn`, the same in every language; a
  document's parse error is its parser's message, after the format: `JSON: ...`.
- `patch` - the GNU diff format, through diffy.

### Tests

Each case in `tests/json`, `tests/txt` and `tests/patch_diff` is a base and two changes
of it, `a` and `b`: with a `result`, the patches of `a` and `b` must not intersect, and
Base + A + B = Base + B + A = result; without one, they must intersect.
`tests/commute` checks the same on random changes, seeded:

```
cargo test --features patch
COMMUTE_SEED=5 COMMUTE_ROUNDS=100000 cargo test --features patch commute
cargo llvm-cov --features patch --summary-only    # coverage is about 95% at least in CI
```
