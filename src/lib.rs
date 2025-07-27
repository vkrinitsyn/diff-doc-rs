// pub mod json_wrapper;
pub mod patch_wrapper;
pub mod txt_impl;
// mod dd;
mod xml_wrapper;

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use diffy::{create_patch, Patch};
use crate::patch_wrapper::is_intersect_txt;
// re-export
// pub use crate::dd::process::{compare_strs, compare_json};
// pub use crate::dd::enums::DiffTreeNode;

trait Mismatch {
    fn new(data1: &String, data2: &String) -> Result<Self, DocParseError> where Self: Sized;

    fn apply(&self, input: &String) -> Result<String, DocParseError>;

    fn mismatch_type(&self) -> MismatchType;
}


#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MismatchType {
    /// with initial diff patch content
    Patch(String),
    /// json document, must start with { or [
    Json,
    /// internal updates algorithm, line number strings update only (for now)
    Text,
    /// Xml document, must start with < and end with >
    Xml,
    /// yaml document: better start with --- to identify as yaml
    Yaml,
}

/**
Support two types of patch files: the
 - Text file format as https://en.wikipedia.org/wiki/Diff
 - document format
*/
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DocMismatch {
    pub mismatch_type: MismatchType,
    /// path to new value or to remove it
    /// values are:
    /// - true | false is bool
    /// - double-quoted for string 
    /// - otherwise considered numeric
    pub diff: HashMap<String, Option<String>>,
}

impl Display for DocMismatch {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), core::fmt::Error> {
        match &self.mismatch_type {
            MismatchType::Patch(text) => {
                let _ = writeln!(f, "{text}")?;
            }
            _ => {
                let _ = writeln!(f, "*** {}", self.mismatch_type)?;
                for (path, value) in &self.diff {
                    let _ = writeln!(f, "@@ {path} @@")?;
                    match value {
                        None => {
                            let _ = writeln!(f, "-")?;
                        }
                        Some(value) => {
                            let _ = writeln!(f, "~{value}")?;
                        }
                    }
                }
            }
        };
        Ok(())
    }
}

impl Display for MismatchType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), core::fmt::Error> {
        writeln!(f, "{}",
            match &self {
                MismatchType::Json => DocMismatch::JSON_FORMAT,
                MismatchType::Patch(_) => "", // wont happens - assert exception
                MismatchType::Xml => DocMismatch::XML_FORMAT,
                MismatchType::Yaml => DocMismatch::YAML_FORMAT,
                MismatchType::Text => DocMismatch::TEXT_FORMAT,
            }
        )
    }
}


impl TryFrom<String> for DocMismatch {
    type Error = core::fmt::Error;

    /// deserialization from text format i.e. patch file
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mismatch_type = DocMismatch::format(&value);
        let mut diff = HashMap::new();
        fn append(diff: &mut HashMap<String, Option<String>>, key: &String, val: &String){
            if key.len()>0 && val.len() > 0 {
                diff.insert(key.to_owned(),
                    if val == "-" { // removing value
                        None
                    } else {
                        Some(val[1..].to_string())
                    });
            }
        }
        if let MismatchType::Patch(_) = &mismatch_type {
            // diff will calculate by patch text, the patch text is required for apply
            return Ok(DocMismatch{mismatch_type, diff});
        }
        let mut lc = 0;
        let mut key = String::new();
        let mut val = String::new();
        for l in value.split("\n").collect::<Vec<&str>>() {
            if l.len() == 0 && lc == 1 {
                continue;
            } else if l.len() > 6 && l.starts_with("@@ ") && l.ends_with(" @@") {
                append(&mut diff, &key, &val);
                lc = 0;
                key = l[3..l.len()-3].to_owned();
                val = String::new();
            } else if key.len() > 0 {
                if val.len() > 0 && l.len() > 0 {
                    // multiline value
                    val.push_str("\n");
                }
                val.push_str(l);
            }
            lc += 1;
        }
        append(&mut diff, &key, &val);

        Ok(DocMismatch{mismatch_type, diff})
    }
}

impl DocMismatch {
    /// supported formats
    const JSON_FORMAT: &'static str = "json";
    const XML_FORMAT: &'static str = "xml ";
    const YAML_FORMAT: &'static str = "yaml";
    const TEXT_FORMAT: &'static str = "text";

    pub fn format(input: &String) -> MismatchType {
        if let Some(l) = input.find("\n") {
            let il = &input[..l];
            if il.len() > 7 && il.starts_with("*** ") {
                match &il[4..8] {
                    Self::JSON_FORMAT => { return MismatchType::Json; }
                    Self::XML_FORMAT => { return MismatchType::Xml; }
                    Self::YAML_FORMAT => { return MismatchType::Yaml; }
                    _ => ()
                }
            }
        }
        MismatchType::Patch(input.clone())
    }

    pub fn is_intersect(&self, b: &DocMismatch) -> Result<bool, DocParseError> {
        if let MismatchType::Patch(a) = &self.mismatch_type {
            if let MismatchType::Patch(b) = &b.mismatch_type {
                return is_intersect_txt(a, b).map_err(|e| DocParseError::new(e.to_string()));
            }
        }

        for (a, v) in &self.diff {
            if b.is_partial_contains(a, v)? {
                return Ok(true);
            }
        }
        for (b, v) in &b.diff {
            if self.is_partial_contains(b, v)? {
                return Ok(true);
            }
        }
        Ok(false)
    }


    pub fn is_partial_contains(&self, path: &String, val: &Option<String>) -> Result<bool, DocParseError> {
        if path.ends_with("]") { // it's array element update
            match path.rfind("[") {
                None => Err(DocParseError::new(format!("expected array, but found {}", path))), 
                Some(idx) => {
                    let idx = path[idx + 1..path.len() - 2].parse::<usize>()
                        .map_err(|e| DocParseError::new(format!("expected array numeric index, but {}", e)))?;
                    Ok(self.diff.iter()
                        .find(|(p, v)| if p.ends_with("]") {
                            match p.rfind("[") {
                                None => { false } // wrong format actually
                                Some(pidx) => match p[pidx + 1..p.len() - 2].parse::<usize>() {
                                    Ok(pidx) => {
                                        if val.is_none() && v.is_none() {
                                            idx != pidx // same delete operation is mutually overlap, unless delete same index
                                        } else if val.is_some() && v.is_some() {
                                            idx == pidx  // same index update is overlap
                                        } else if val.is_none() {
                                            idx > pidx   // only one delete this idx, should be 
                                        } else {
                                            false
                                        }
                                    }
                                    Err(_) => { false } // wrong format actually
                                   }
                                }
                          } else { 
                              false 
                          }
                      )
                      .is_some())
                          // None => Ok(false), // same deletion
              }
          }  
        } else {
            if let MismatchType::Text = self.mismatch_type {
                //
            }
            // let p = self.diff.get(path);
            let base = self.diff.get(path).map(|v| v != val);
            Ok(base.unwrap_or(false)
                || self.diff.iter().find(|(s, v)|
                    path.starts_with(*s) && v!=&val).is_some()) // same key not found, but portion?)
        }
    }
    
    pub fn guess_doc_format(input: &String) -> MismatchType {
        if input.starts_with("{") || input.starts_with("[") { 
            MismatchType::Json 
        } else if input.starts_with("<") &&
            (input.ends_with(">") || input.ends_with(">\n")) {
            MismatchType::Xml
        } else if input.starts_with("--- ") {
            MismatchType::Yaml
        } else {
            MismatchType::Patch("".to_string())
        }
    }

    pub fn new(base_a: &String, base_b: &String, mismatch_type: MismatchType) -> Result<Self, DocParseError> {
        match &mismatch_type {
            MismatchType::Patch(_) =>  Ok(Self { mismatch_type, diff: HashMap::new()}),
            MismatchType::Text =>  DocMismatch::new_txt(base_a, base_b),
            MismatchType::Xml => DocMismatch::new_xml(base_a, base_b),
            _ => {
                todo!()
            },
        }
    }
    
    /// compare two documents
    pub fn new_guess(base_a: &String, base_b: &String) -> Result<Self, DocParseError> {
        let b = Self::guess_doc_format(base_b);
        match Self::guess_doc_format(base_a) {
            MismatchType::Patch(_) =>  match &b {
                MismatchType::Patch(_) => Ok(Self { mismatch_type: MismatchType::Patch(create_patch(base_a, base_b).to_string()), diff: HashMap::new() }),
                _ => Err(DocParseError::new(format!("unmatched types: Txt and {}", b)))
            }
            MismatchType::Json => match &b {
                MismatchType::Json => Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Json and {}", b)))
            }
            MismatchType::Text => match &b {
                MismatchType::Text => Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Json and {}", b)))
            }

            MismatchType::Xml =>  match &b {
                MismatchType::Xml =>  Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Xml and {}", b)))
            }
            MismatchType::Yaml =>  match &b {
                MismatchType::Yaml =>  Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Yaml and {}", b)))
            }

        }
    }

    pub fn apply(&self, base: &String) -> Result<String, DocParseError> {
        match &self.mismatch_type {
            // MismatchType::Json => self.apply_json_str(base),
            MismatchType::Text => self.apply_txt(base),
            MismatchType::Patch(p) => // better to parse patch, check for intersection and then call diffy::apply
                diffy::apply(base, &Patch::from_str(p.as_str())
                    .map_err(|e| DocParseError::new(e.to_string()))?)
                    .map_err(|e| DocParseError::new(e.to_string())),
            MismatchType::Xml => self.apply_xml(base),
            _ => {
                todo!()
            },
        }
    }
}

#[derive(Debug)]
pub struct DocParseError(Cow<'static, str>);

impl DocParseError {
    fn new<E: Into<Cow<'static, str>>>(e: E) -> Self {
        Self(e.into())
    }
}
impl std::error::Error for DocParseError {}

impl fmt::Display for DocParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

mod tests {
    // #[tokio::test]
    #[test]
    fn test_compile() {
        assert!(true);
    }
}