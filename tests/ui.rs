//! Compile-fail tests for `#[command]` (including its `# Arguments` section),
//! `#[group]`, `commands!`, `cli!`, `#[derive(Arg)]` (both modes) and
//! `#[derive(Choice)]`, and for the `Arg`, `ArgHelp` and `Choice` traits'
//! `on_unimplemented` messages.
//!
//! A macro's rejections are part of its contract: the *message* is the whole
//! documentation a user gets at the moment they need it, and the *span* is what
//! sends them to the right token. Both are asserted here, one file per
//! rejection, against the recorded `.stderr` next to it.
//!
//! After an intentional change to a message — a macro's constant, or a trait's
//! `#[diagnostic::on_unimplemented]` note — re-record with `just ui-bless`,
//! then read every changed `.stderr` as if you were the user who hit it: the
//! bless records whatever the compiler printed, right or wrong.

#[test]
fn rejections_explain_themselves() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
