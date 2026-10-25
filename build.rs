// [i18n] rust-i18n reads locales/ as the crate compiles: a message edited there rebuilds it.
// Without the i18n feature the messages are English only: their `en:` lines, as the
// table en.rs in OUT_DIR, which src/i18n.rs includes
use std::fmt::Write;

fn main() {
    println!("cargo:rerun-if-changed=locales");
    let mut files: Vec<_> = std::fs::read_dir("locales").unwrap().map(|f| f.unwrap().path()).collect();
    files.sort();
    let mut table = String::from("fn en(key: &str) -> &'static str {\n    match key {\n");
    for f in files {
        let mut key = None;
        for line in std::fs::read_to_string(&f).unwrap().lines() {
            if !line.starts_with([' ', '#']) && line.ends_with(':') {
                key = Some(line.trim_end_matches(':').to_string());
            } else if let (Some(k), Some(m)) = (&key, line.trim().strip_prefix("en: \"")) {
                let m = m.strip_suffix('"').unwrap_or_else(|| panic!("{}: {}: not in quotes", f.display(), k));
                assert!(!m.contains('\\'), "{}: {}: no escapes in a message", f.display(), k);
                writeln!(table, "        {:?} => {:?},", k, m).unwrap();
            }
        }
    }
    table.push_str("        _ => \"\",\n    }\n}\n");
    std::fs::write(format!("{}/en.rs", std::env::var("OUT_DIR").unwrap()), table).unwrap();
}
