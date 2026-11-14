//! The Rust types of `proto/diff_doc.proto` (feature `proto`): what a gRPC service sends a
//! [`Mismatches`] as - another protocol's message imports diff_doc.proto and takes these
//! types (prost-build's `extern_path(".diff_doc", "::diff_doc::proto")`) instead of
//! compiling a copy of its own.
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

include!("proto_types.rs");

use mismatches::Kind;

impl fmt::Display for Mismatches {
	/// the text of the mismatches: a patch as it is, a document's hunks one per line
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.text())
	}
}

impl Mismatches {
	/// Feed the mismatches into a hasher: the same mismatches hash the same.
	#[allow(clippy::should_implement_trait)] // the std trait takes another signature
	pub fn hash(&self, h: &mut DefaultHasher) {
		if let Some(m) = &self.kind {
			match m {
				Kind::Patch(s) => {
					h.write_u8(1);
					s.hash(h);
				}
				Kind::Doc(doc) => {
					h.write_u8(2);
					for hunk in &doc.hunks {
						for idx in &hunk.path {
							if let Some(k) = &idx.kind {
								match k {
									doc_index::Kind::Name(n) => n.hash(h),
									doc_index::Kind::Idx(i) => i.hash(h),
								}
							}
						}
					}
				}
				Kind::Text(txt) => {
					h.write_u8(3);
					for op in &txt.ops {
						if let Some(k) = &op.kind {
							match k {
								diff_op::Kind::Remove(r) => {
									h.write_u8(1);
									r.index.hash(h);
								}
								diff_op::Kind::Insert(i) => {
									h.write_u8(2);
									i.index.hash(h);
									i.value.hash(h);
								}
								diff_op::Kind::Update(u) => {
									h.write_u8(3);
									u.index.hash(h);
									u.value.hash(h);
								}
								diff_op::Kind::Append(a) => {
									h.write_u8(4);
									a.index.hash(h);
									a.pos.hash(h);
									a.value.hash(h);
								}
							}
						}
					}
				}
			}
		}
	}

	fn text(&self) -> String {
		match &self.kind {
			None => String::new(),
			Some(m) => match m {
				Kind::Patch(s) => s.clone(),
				Kind::Doc(doc) => {
					let hunks: Vec<String> = doc.hunks.iter().map(|hunk| {
						let path: Vec<String> = hunk.path.iter().map(|idx| {
							match &idx.kind {
								Some(doc_index::Kind::Name(n)) => n.clone(),
								Some(doc_index::Kind::Idx(i)) => i.to_string(),
								None => "_".into(),
							}
						}).collect();
						path.join(".")
					}).collect();
					format!("doc[{}]", hunks.join(", "))
				}
				Kind::Text(txt) => {
					let ops: Vec<String> = txt.ops.iter().map(|op| {
						match &op.kind {
							Some(diff_op::Kind::Remove(r)) => format!("-{}", r.index),
							Some(diff_op::Kind::Insert(i)) => format!("+{}:{}", i.index, i.value),
							Some(diff_op::Kind::Update(u)) => format!("~{}:{}", u.index, u.value),
							Some(diff_op::Kind::Append(a)) => format!("^{}:{}:{}", a.index, a.pos, a.value),
							None => "?".into(),
						}
					}).collect();
					format!("txt[{}]", ops.join(", "))
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use prost::Message;

	fn doc() -> Mismatches {
		Mismatches { kind: Some(Kind::Doc(DocMismatch { hunks: vec![Hunk {
			path: vec![DocIndex { kind: Some(doc_index::Kind::Name("a".into())) }, DocIndex { kind: Some(doc_index::Kind::Idx(2)) }],
			value: None,
		}] })) }
	}

	#[test]
	fn mismatches_print_as_their_text() {
		assert_eq!(Mismatches { kind: Some(Kind::Patch("@@ -1 +1 @@".into())) }.to_string(), "@@ -1 +1 @@");
		assert_eq!(doc().to_string(), "doc[a.2]");
		assert_eq!(Mismatches::default().to_string(), "");
	}

	#[test]
	fn mismatches_go_on_the_wire_and_back_and_hash_alike() {
		let m = doc();
		assert_eq!(Mismatches::decode(m.encode_to_vec().as_slice()).unwrap(), m);
		let (mut a, mut b) = (DefaultHasher::new(), DefaultHasher::new());
		m.hash(&mut a);
		Mismatches::decode(m.encode_to_vec().as_slice()).unwrap().hash(&mut b);
		assert_eq!(a.finish(), b.finish());
		let mut c = DefaultHasher::new();
		Mismatches { kind: Some(Kind::Patch("x".into())) }.hash(&mut c);
		assert_ne!(a.finish(), c.finish());
	}
}
