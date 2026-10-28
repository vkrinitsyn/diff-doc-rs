# Contributing to diff-doc-rs (diff_doc)

Thank you for helping. diff_doc is MIT licensed; a contribution is accepted under the
same license (see [LICENSE](LICENSE)).

## Issues

Open an issue on GitHub for a bug or a feature request. For a bug, give the diff_doc
version, the features it is built with (`patch`, `bson`, `--no-default-features`), the
format of the documents (JSON, YAML, TOML, XML, BSON, text or a GNU patch), and the
smallest base and changes that show it: what the patches were, what you expected, and
what you got.

A security problem is not reported in a public issue: write to
**support@dbinvent.com** instead.

## Pull requests

1. Fork the repository and branch from `main`.
2. Keep a change to one subject; describe what it changes and why.
3. Build and test it (below); add tests for what you change. A 3-way case is a
   directory in `tests/json`, `tests/txt` or `tests/patch_diff`: a base, `a` and `b`,
   and a `result` when their patches must not intersect (see the README).
4. Messages a user reads go through the locale file (`locales/error.yml`): add a new
   key in every language there, with its `DD-` code.
5. Open the pull request against `main`.

## Build and test

```
cargo test                               # default features (i18n)
cargo test --no-default-features         # English only
cargo test --features patch,bson         # the GNU patch and BSON too
cargo llvm-cov --features patch,bson --summary-only   # CI fails under 90% of lines
make -C c test BSON=1                    # the C library (protobuf-c), its BSON too
```

A change to the BSON mapping is made on both sides, `src/bson_doc.rs` and
`c/bson_doc.c`: `BSON_FIXTURE=write cargo test --features bson the_c_fixture` writes
`c/testdata` anew, and the C test holds `c/bson_doc.c` to it.

Before a pull request, run the checks as well (`cargo make audit`, `cargo make geiger`):

```
cargo audit      # known vulnerabilities in the pinned dependencies
cargo geiger     # unsafe code in diff_doc and its dependencies
```

diff_doc forbids unsafe code of its own (`#![forbid(unsafe_code)]`).

## Code of conduct

Taking part in this project means following its
[Code of Conduct](CODE_OF_CONDUCT.md).
