//! Completion, end to end: from `#[arg(complete = …)]` on a type, through
//! `#[command]`, to the candidate values a shell would be offered.
//!
//! Everything here runs in-process through [`App::complete`], which pins
//! `bpaf`'s revision-1 wire protocol — revision 2 asked for through `run_inner`
//! terminates the process, which would take the test runner with it.
//!
//! The claim under test is **inheritance**: `Id` says once how it completes, and
//! every command that takes an `Id` gets it, with nothing written at the
//! command. The two commands below are deliberately identical in that respect
//! and differ only in name.

use hicli::bpaf::{pure, Bpaf};
use hicli::{app, cli, command, App, Arg, Candidate};

/// The shared context: nothing here runs a command, only parses and completes.
pub struct Ctx;

/// An event id.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_ids)]
pub struct Id(#[bpaf(positional("ID"))] String);

/// The ids on offer, filtered by what has been typed so far.
///
/// The filtering is the completer's job: `bpaf` passes this output through
/// unchanged (it prefix-filters only the flag names it derives itself), so a
/// completer that ignores `typed` offers everything at every keystroke.
fn complete_ids(typed: &Id) -> Vec<Candidate> {
    ["alice", "alfred", "bo"]
        .iter()
        .filter(|c| c.starts_with(typed.0.as_str()))
        .map(|c| ((*c).to_string(), Some("a person".to_string())))
        .collect()
}

/// A directory, completed by the shell itself.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]
pub struct Home(#[bpaf(positional("DIR"))] String);

/// show a person
#[command]
pub fn show(id: Id, ctx: &Ctx) {
    let _ = (id, ctx);
}

// Nothing below mentions ids, candidates or completion — which is the point.
/// greet a person
#[command]
pub fn greet(id: Id, ctx: &Ctx) {
    let _ = (id, ctx);
}

/// store things somewhere
#[command]
pub fn store(home: Home, ctx: &Ctx) {
    let _ = (home.0, ctx);
}

/// The whole tree.
fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [show, greet, store],
    }
}

#[test]
fn a_partial_word_offers_what_matches_it() {
    assert_eq!(app().complete(cli! { show al }), ["alice", "alfred"]);
}

#[test]
fn a_word_that_matches_nothing_offers_nothing() {
    let none: [&str; 0] = [];
    assert_eq!(app().complete(cli! { show zz }), none);
}

#[test]
fn the_empty_word_offers_everything() {
    // A newtype over `String` parses the empty word, so the completer runs from
    // the first keystroke — a newtype over `u64` would not, and the first TAB
    // would yield nothing.
    assert_eq!(app().complete(cli! { show "" }), ["alice", "alfred", "bo"]);
}

#[test]
fn a_second_command_taking_the_same_type_completes_identically() {
    // `greet` declares no completion of its own. The candidates travel with
    // `Id`, so a command written tomorrow inherits them too.
    assert_eq!(
        app().complete(cli! { greet al }),
        app().complete(cli! { show al })
    );
    assert_eq!(app().complete(cli! { greet al }), ["alice", "alfred"]);
}

#[test]
fn a_shell_directive_parses_normally_and_reports_no_values() {
    // `complete = dirs` is an instruction to the shell, not a list of values,
    // and revision 1 of the wire protocol cannot express one — so there is
    // nothing for `App::complete` to report and an empty result here means
    // "the shell's job", not "nothing applicable". What is assertable is that
    // attaching it left the parser alone.
    let (_g, invocation) = app().parse(cli! { store src }).expect("parses");
    assert_eq!(invocation.path(), ["store"]);

    let none: [&str; 0] = [];
    assert_eq!(app().complete(cli! { store "" }), none);
}
