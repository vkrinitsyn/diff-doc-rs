use std::collections::{HashMap, HashSet};
use crate::{DocMismatch, DocParseError, MismatchType};


impl DocMismatch {
    pub fn new_yaml(data1: &String, data2: &String) -> Result<Self, DocParseError> {
        
        todo!()
    }
    
    pub fn apply_jaml(&self, input: &String) -> Result<String, DocParseError> {

        todo!()
    }
}
