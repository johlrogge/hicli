//! `#[macro_use] extern crate hicli;` still brings `cli!` into scope.
//!
//! `cli!` was a `#[macro_export]` `macro_rules!` until 0.5, and is a
//! re-exported proc macro since (ADR-017). A crate root that imported it the
//! 2015 way must keep compiling: this file is that crate root.

#[macro_use]
extern crate hicli;

#[test]
fn macro_use_imports_cli() {
    let v: Vec<String> = cli! { set-output };
    assert_eq!(v, ["set-output"]);
}
