//! `#[group]` on a file module, as a consumer writes it: one group, one file.
//!
//! This is `tests/groups.rs`'s tree with its bodies moved out of the file. The
//! declarations below are the whole registration — no list, no `mod.rs`
//! ceremony, no `#[path]` to maintain — and the three files under
//! `tests/group_files/` are ordinary Rust that rustfmt formats and
//! rust-analyzer follows.
//!
//! The tree, and what each piece is here to prove:
//!
//! ```text
//! event                       a file module, resolved into `tests/group_files/`
//!   create                    a command in a file
//!   roster                    #[cfg(any())] — compiled out, and unregistered with it
//!   instance                  an *inline* #[group] inside a file module
//!     branch
//!   audience                  a file #[group] nested one level down
//!     define
//!   gone                      a #[cfg(any())] file #[group] with no file at all
//! id                          #[group(ctx = Ctx, alias = …)] on the declaration
//!   create
//! tools                       an author-written #[path] on the declaration
//!   version                   no context anywhere: a generic handle
//! ```
//!
//! `tests/group_files/` is not a test target — cargo only globs `tests/*.rs` —
//! so the files in it are compiled exactly once, as this crate's modules.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{pure, Bpaf};
use hicli::{app, cli, group, App, Arg, Command, Failure};

// --- the consumer's context and argument structs ------------------------------

/// The shared context. It records what ran, so a run-level test can assert
/// *that* a command ran.
pub struct Ctx {
    log: Rc<RefCell<Vec<String>>>,
}

impl Ctx {
    /// Record one line of evidence.
    pub fn note(&self, line: impl Into<String>) {
        self.log.borrow_mut().push(line.into());
    }
}

/// A fresh recording log.
fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

/// An `mk_ctx` recording into `log`. The globals are `()`.
fn ctx_from(log: &Rc<RefCell<Vec<String>>>) -> impl FnOnce(()) -> Result<Ctx, Failure> {
    let log = Rc::clone(log);
    move |()| Ok(Ctx { log })
}

/// What ran, in order.
fn ran(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    log.borrow().clone()
}

/// A one-positional argument struct; its bpaf constructor is `title()`, and
/// `#[derive(Arg)]` is what carries that across a file boundary — the commands
/// in `group_files/` import the type, and nothing else.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Title(#[bpaf(positional("TITLE"))] String);

// --- the tree -----------------------------------------------------------------

// This file is `tests/group_files.rs` and has a `tests/group_files/` directory
// beside it, so the declarations resolve into that directory and the macro
// emits the `#[path]` that makes it so. (`tests/group_files.rs` is a crate
// root, which rustc would resolve against `tests/` instead — the emitted
// `#[path]` is what removes the disagreement.)

/// events and instances
///
/// An event is a directory; an instance is a branch of it.
#[group]
pub mod event;

// `id`'s command names its context as `super::Ctx`, which cannot be lifted into
// this scope by token — so the group says what it is. Its alias is read off the
// same attribute.
/// identity
#[group(ctx = Ctx, alias = "ident")]
pub mod id;

// An author-written `#[path]` wins, and is relative to *this* file's directory.
// The macro synthesises nothing here.
/// the odds and ends
#[path = "group_files/odds.rs"]
#[group]
pub mod tools;

/// The whole tree, declared.
fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [event, id, tools],
    }
}

// --- membership, at every level -----------------------------------------------

#[test]
fn a_function_in_the_file_is_a_command_in_the_group() {
    let (_g, invocation) = app()
        .parse(cli! { event create "Summer Party" })
        .expect("parses");
    assert_eq!(invocation.path(), ["event", "create"]);
}

#[test]
fn an_inline_group_inside_a_file_module_is_a_child() {
    // `instance` is an ordinary `#[group] mod` written *in* `event.rs`: the two
    // forms nest in either order, because a child is a child because it is in
    // the module.
    let (_g, invocation) = app()
        .parse(cli! { event instance branch Rehearsal })
        .expect("parses");
    assert_eq!(invocation.path(), ["event", "instance", "branch"]);
}

#[test]
fn a_file_group_nests_one_file_deeper() {
    // `event.rs` declares `audience`, which resolves against *that* file —
    // `tests/group_files/event/audience.rs` — because `event.rs` is not a
    // mod-rs file and has an `event/` directory of its own.
    let (_g, invocation) = app()
        .parse(cli! { event audience define Friends })
        .expect("parses");
    assert_eq!(invocation.path(), ["event", "audience", "define"]);
}

#[test]
fn a_declared_context_type_is_the_groups_context() {
    // Without `#[group(ctx = Ctx)]` on the declaration, `id`'s only command names
    // `&super::Ctx` — a type that means something else one module up — and the
    // group would refuse to compile.
    let c: Command<Ctx> = __hicli_id();
    assert_eq!(c.name(), "id");

    let (_g, invocation) = app().parse(cli! { id create }).expect("parses");
    assert_eq!(invocation.path(), ["id", "create"]);
}

#[test]
fn a_declarations_alias_reaches_the_file_group() {
    // `#[group(alias = …)]` on a file module means what it means on an inline
    // module: another spelling, the canonical path.
    let (_g, invocation) = app().parse(cli! { ident create }).expect("parses");
    assert_eq!(invocation.path(), ["id", "create"]);
}

#[test]
fn an_authors_own_path_finds_the_file() {
    // `tools` lives in `odds.rs`: the name of the module and the name of the
    // file are allowed to differ, and the macro does not second-guess it.
    let (_g, invocation) = app().parse(cli! { tools version }).expect("parses");
    assert_eq!(invocation.path(), ["tools", "version"]);
}

#[test]
fn a_context_free_file_group_mounts_in_any_app() {
    // Nothing in `odds.rs` names a context, so `tools`'s handle is generic —
    // the same rule the inline form follows, reached through a file.
    let elsewhere: App<(), String> = app! { globals: pure(()), commands: [tools] };
    let (_g, invocation) = elsewhere.parse(cli! { tools version }).expect("parses");
    assert_eq!(invocation.path(), ["tools", "version"]);
}

// --- the run path -------------------------------------------------------------

#[test]
fn a_command_three_levels_down_runs() {
    let log = log();
    let code = app().run_with_args(cli! { event audience define Friends }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["define:Friends"]);
}

#[test]
fn a_command_in_an_inline_group_in_a_file_runs() {
    let log = log();
    let code = app().run_with_args(cli! { event instance branch Rehearsal }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["branch:Rehearsal"]);
}

// --- what is *not* a child ----------------------------------------------------

#[test]
fn a_configured_out_command_in_a_file_is_not_registered() {
    // `#[cfg(any())]` rode from the function in `event.rs` onto its own `push`
    // in this file's handle — the same rule, across a file boundary.
    let e = app()
        .parse(cli! { event roster })
        .err()
        .expect("`roster` is not compiled, so it is not a command");
    assert_eq!(hicli::exit_code_for(&e), 2);

    let e = app()
        .parse(cli! { event --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    assert!(
        !out.contains("roster"),
        "a cfg'd-out command is listed:\n{out}"
    );
}

#[test]
fn a_configured_out_file_group_is_not_registered_and_needs_no_file() {
    // `gone` in `event.rs` is `#[cfg(any())]` and has no file anywhere: the
    // crate compiling at all is half the proof, and this is the other half.
    let e = app()
        .parse(cli! { event gone })
        .err()
        .expect("`gone` is not compiled, so it is not a group");
    assert_eq!(hicli::exit_code_for(&e), 2);

    let e = app()
        .parse(cli! { event --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    assert!(
        !out.contains("gone"),
        "a cfg'd-out file group is listed:\n{out}"
    );
}

// --- help ---------------------------------------------------------------------

#[test]
fn a_declarations_doc_comment_is_the_groups_help() {
    let e = app()
        .parse(cli! { --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    for word in ["event", "events and instances", "id", "identity", "tools"] {
        assert!(out.contains(word), "help missing `{word}`:\n{out}");
    }
}

#[test]
fn the_rest_of_a_declarations_doc_comment_becomes_details() {
    let e = app()
        .parse(cli! { event --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    assert!(
        out.contains("An event is a directory; an instance is a branch of it."),
        "group help missing its details:\n{out}"
    );
}
