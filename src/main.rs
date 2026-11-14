#![forbid(unsafe_code)]
// use diff_doc::*;

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
    #[allow(clippy::assertions_on_constants)] // a build check: it passes when the crate compiles
    fn test_compile() {
        assert!(true);
    }
}