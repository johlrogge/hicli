//! Compile-fail tests for `#[command]`, `#[group]`, `commands!` and `cli!`.
//!
//! A macro's rejections are part of its contract: the *message* is the whole
//! documentation a user gets at the moment they need it, and the *span* is what
//! sends them to the right token. Both are asserted here, one file per
//! rejection, against the recorded `.stderr` next to it.
//!
//! After an intentional change to a message, re-record with `just ui-bless`
//! and read the diff as if you were the user who hit it.

#[test]
fn rejections_explain_themselves() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
