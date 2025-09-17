use serde_json::json;
use diff_doc::*;
use diff_doc::diff::Mismatch;

/**
usage examples

*/

fn main() {
    // let m = Mismatch::new( &json!({"a":["b","c"], "d":["e","f"]}),
    //                           &json!({"a":["b","c"], "d":["e","g"]})).unwrap();
    // println!("{}", m);

}


#[cfg(test)]
mod tests {
    #[test]
    fn test_compile() {
        assert!(true);
    }
}