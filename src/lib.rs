pub mod json_wrapper;
pub mod txt_wrapper;
mod dd;
mod xml_wrapper;
mod jaml_wrapper;

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use diffy::{create_patch, Patch};
use crate::txt_wrapper::is_intersect_txt;
// re-export
pub use crate::dd::process::compare_strs;


pub enum MismatchType {
    /// json documet, must start with { or [ 
    Json,
    /// with initial diff patch content
    Text(String),
    /// Xml document, must start with < and end with >
    Xml,
    /// yaml document: better start with --- to identify as yaml
    Yaml,
}
/**
Text file format is a https://en.wikipedia.org/wiki/Diff
but if first line instead of standard `*** /path/to/original `

use `*** json` OR `*** xml` OR `*** yaml` to specify document type

*/
pub struct DocMismatch {
    pub mismatch_type: MismatchType,
    /// path to new value or to remove it
    pub diff: HashMap<String, Option<String>>,
}

impl Display for DocMismatch {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), core::fmt::Error> {
        match &self.mismatch_type {
            MismatchType::Text(text) => {
                let _ = writeln!(f, "{}", text)?;
            }
            _ => {
                let _ = writeln!(f, "*** {}", self.mismatch_type)?;
                for (path, value) in &self.diff {
                    let _ = writeln!(f, "@@ {} @@", path)?;
                    match value {
                        None => {
                            let _ = writeln!(f, "-")?;
                        }
                        Some(value) => {
                            let _ = writeln!(f, "~{}", value)?;
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
                MismatchType::Text(_) => "", // wont happens
                MismatchType::Xml => DocMismatch::XML_FORMAT,
                MismatchType::Yaml => DocMismatch::YAML_FORMAT,
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
        if let MismatchType::Text(_) = &mismatch_type {
            // diff will calculate by patch text, the patch text is required for apply
            return Ok(DocMismatch{mismatch_type, diff});
        }
        let mut lc = 0;
        let mut key = String::new();
        let mut val = String::new();
        for l in value.split("\n").collect::<Vec<&str>>() {
            if l.len() > 6 && &l[0..4] == "@@ "&& &l[l.len()-3..] == " @@" {
                if lc > 0 && key.len()>0 && val.len() > 0 {
                    diff.insert(key.to_owned(),
                    if lc == 1 && l == "-" { // removing value
                        None
                    } else {
                        Some(val[1..].to_string())
                    });
                }
                lc = 0;
                key = l[4..l.len()-3].to_string();
                val = String::new();
            } else {
                if val.len() > 0 {
                    // multiline value
                    val.push_str("\n");
                }
                val.push_str(l);
            }
            lc += 1;
        }

        Ok(DocMismatch{mismatch_type, diff})
    }
}

impl DocMismatch {
    /// supported formats
    const JSON_FORMAT: &'static str = "json";
    const XML_FORMAT: &'static str = "xml ";
    const YAML_FORMAT: &'static str = "yaml";

    pub fn format(input: &String) -> MismatchType {
        if let Some(l) = input.find("\n") {
            let il = &input[..l];
            if il.len() > 7 && &il[0..5] == "*** " {
                match &il[4..8] {
                    Self::JSON_FORMAT => { return MismatchType::Json; }
                    Self::XML_FORMAT => { return MismatchType::Xml; }
                    Self::YAML_FORMAT => { return MismatchType::Yaml; }
                    _ => ()
                }
            }
        }
        MismatchType::Text(input.clone())
    }

    pub fn is_intersect(&self, b: &DocMismatch) -> Result<bool, DocParseError> {
        if let MismatchType::Text(a) = &self.mismatch_type {
            if let MismatchType::Text(b) = &b.mismatch_type {
                return is_intersect_txt(a, b).map_err(|e| DocParseError::new(e.to_string()));
            }
        }

        for (a, _) in &self.diff {
            if b.is_partial_contains(a) {
                return Ok(true);
            }
        }
        for (b, _) in &b.diff {
            if self.is_partial_contains(b) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn is_partial_contains(&self, path: &String) -> bool {
        if self.diff.contains_key(path) {
            return true;
        }
        for (s, _) in &self.diff {
            if path.starts_with(s) {
                return true;
            }
        }
        false
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
            MismatchType::Text("".to_string())
        }
    }

    pub fn new(base_a: &String, base_b: &String, mismatch_type: MismatchType) -> Result<Self, DocParseError> {
        match &mismatch_type {
            MismatchType::Json => DocMismatch::new_json(base_a, base_b),
            MismatchType::Text(_) =>  Ok(Self { mismatch_type, diff: HashMap::new()}),
            MismatchType::Xml => DocMismatch::new_xml(base_a, base_b),
            MismatchType::Yaml =>  DocMismatch::new_yaml(base_a, base_b),
        }
    }
    
    /// compare two documents
    pub fn new_guess(base_a: &String, base_b: &String) -> Result<Self, DocParseError> {
        let b = Self::guess_doc_format(base_b);
        match Self::guess_doc_format(base_a) {
            MismatchType::Json => match &b {
                MismatchType::Json => Self::new(base_a, base_b, b),
                _ => Err(DocParseError::new(format!("unmatched types: Json and {}", b)))
            }
            MismatchType::Text(_) =>  match &b {
                MismatchType::Text(_) => Ok(Self { mismatch_type: MismatchType::Text(create_patch(base_a,  base_b).to_string()), diff: HashMap::new() }),
                _ => Err(DocParseError::new(format!("unmatched types: Txt and {}", b)))
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
            MismatchType::Json => self.apply_json_str(base),
            MismatchType::Text(p) => // better to parse patch, check for intersection and then call diffy::apply 
                diffy::apply(base, &Patch::from_str(p.as_str())
                    .map_err(|e| DocParseError::new(e.to_string()))?)
                    .map_err(|e| DocParseError::new(e.to_string())),
            MismatchType::Xml => self.apply_xml(base),
            MismatchType::Yaml => self.apply_jaml(base),
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
    #[tokio::test]
    async fn test_compile() {
        assert!(true);
    }
}