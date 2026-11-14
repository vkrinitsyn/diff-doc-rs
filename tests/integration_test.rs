mod json;
mod txt;
mod commute;
mod formats;
mod apply;
mod explain;
#[cfg(feature="patch")] mod patch_diff;

#[test]
#[allow(clippy::assertions_on_constants)] // a build check: it passes when the crate compiles
fn test_it() {
    assert!(true);
}