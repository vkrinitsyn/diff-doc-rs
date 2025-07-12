## diff-doc-rs

### Calculate diff on structured documents with patch file apply.

Also support standard diff for text files.

The main goal is to provide Rust lib for a 3-way compare-apply for implement commutative style document change:

> Base + patch_A + patch_B = Base + patch_B + patch_A

### Support documents type:

- [x] Json - diff-like file format, wrapper to json_diff_ng with additional apply diff features
- [x] Txt - plain text document with default diff file format, wrapper to diffy
- [ ] XML
- [ ] Yaml

