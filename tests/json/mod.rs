extern crate diff_doc;

use serde_json::json;
use diff_doc::DocMismatch;
use crate::diff_doc::*;

#[tokio::test]
    async fn test_json_arrays() {
        let data1 = json!(["a", {"c": ["d","f"] }, "b"]);
        let data2 = json!(["b", {"c": ["e","d"] }, "a"]);
    
        // !! todo fixme 
    // @@ .[2] @@
    // ~"a"
    // @@ .[1] @@
    // ~{"c":["e","d"]}
    // @@ .[0] @@
    // -
    // fix : .[0] = ~"b"
    // fix : .[1].c.[0] = ~"e"
    // fix : .[1].c.[1] = ~"d"

        let diffs = compare_json(&data1, &data2, &[]).unwrap();
        assert_eq!(diffs.left_only, DiffTreeNode::Null);
        assert_eq!(diffs.right_only, DiffTreeNode::Null);
        assert_ne!(diffs.unequal_values, DiffTreeNode::Null);
        let m = DocMismatch::from(diffs);
        println!("{}", m);

        assert_eq!(m, DocMismatch::try_from(m.to_string()).unwrap());
    
        /*
        println!("{}", DocMismatch::from(compare_strs(r#"["a"]"#, r#"["b"]"#, true, &[]).unwrap()));
        println!("{}", DocMismatch::from(compare_strs("{}", r#"{"a":"1"}"#,  true, &[]).unwrap()));
        println!("{}", DocMismatch::from(compare_strs("{\"a\":\"1\"}", "{\"b\":\"1\"}", true, &[]).unwrap()));
        */
    
        // assert!(!diffs.is_empty());
        // let diffs = diffs.unequal_values.get_diffs();
        // 
        // assert_eq!(diffs.len(), 1);
        // assert_eq!(
        //     diffs.first().unwrap().to_string(),
        //     r#".[0].c.[1].("f" != "e")"#
        // );
    }

/*
    #[tokio::test]
    async fn test_json_diff_ng2() {
        let data1 = r#"{"a":"b", "c": "df" }"#;
        let data2 = r#"{"a":"b", "c": "ed" }"#;
        let diffs = compare_strs(data1, data2, false, &[]);
        let p1 = extract_path(&diffs);
        for p in &p1 {
            // println!("{} {}", dt, de);
            println!("{}", p);
        }
        println!("second pair");

        let data1 = r#"{"aa":{"a":"b", "c": "d" }, "bb": {"a":"b", "c": "d" }}"#;
        let data2 = r#"{"aa":{"a":"ce", "c": "de" }, "bb": {"a":"b", "c": "d" }}"#;
        let diffs = compare_strs(data1, data2, false, &[]);
        let p2 = extract_path(&diffs);
        for p in &p2 {
            // println!("{} {}", dt, de);
            println!("{}", p);
        }
        
        println!("{} ", is_intersect(&p1, &p2));

    }

    #[tokio::test]
    async fn test_json_diff_ng_i1() {
        assert!(is_intersect(&extract_path(&compare_strs(
            r#"{"a":"b", "c": "df" }"#,
            r#"{"a":"b", "c": "ed" }"#, false, &[])),
        &extract_path(&compare_strs(
             r#"{"a":"b", "c": "df" }"#,
             r#"{"a":"b", "c": "ex" }"#, false, &[]))
        ));
    }

    #[tokio::test]
    async fn test_json_diff_ng_i2() {
        assert!(!is_intersect(&extract_path(&compare_strs(
            r#"{"a":"b", "c": "df" }"#,
            r#"{"a":"b", "c": "ed" }"#, false, &[])),
        &extract_path(&compare_strs(
             r#"{"a":"b", "c": "df" }"#,
             r#"{"a":"a", "c": "df" }"#, false, &[]))
        ));
    }

    #[tokio::test]
    async fn test_json_diff_ng_i3() {
        assert!(!is_intersect(&extract_path(&compare_strs(
            r#"{"a":"b", "c": "df" }"#,
            r#"{"a":"b" }"#, false, &[])),
        &extract_path(&compare_strs(
             r#"{"a":"b", "c": "df" }"#,
             r#"{"c": "df" }"#, false, &[]))
        ));
    }

    #[tokio::test]
    async fn test_json_diff_ng_i4() {
        assert!(is_intersect(&extract_path(&compare_strs(
            r#"{"a":"b", "c": "df" }"#,
            r#""#, false, &[])),
        &extract_path(&compare_strs(
             r#"{"a":"b", "c": "df" }"#,
             r#"{"c": "df" }"#, false, &[]))
        ));
    }

    #[tokio::test]
    async fn test_json_diff_ng_i5() {
        assert!(is_intersect(&extract_path(&compare_strs(
            r#"{"a":"b", "c": "df" }"#,
            r#"{"a":"b" }"#, false, &[])),
          &extract_path(&compare_strs(
              r#"{"a":"b", "c": "df" }"#,
              r#""#, false, &[]))
        ));
    }
*/

#[test]
fn test_it() {
    assert!(true);
}