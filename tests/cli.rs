//! End-to-end tests for `verbandi`, written the way the user types.
//!
//! The consumer app mirrors `examples/demo.rs`: globals with `--home`, an
//! `event` group with `create`/`list` leaves, and a top-level `status` leaf.

use verbandi::bpaf::{Bpaf, Parser};
use verbandi::{cli, exit_code_for, verb_test, App, BoxedVerb, Exit, Group, Verb};

struct Ctx;

#[derive(Debug, Clone, Bpaf)]
struct Globals {
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
    #[allow(dead_code)]
    home: String,
}

#[derive(Debug, Clone, Bpaf)]
struct Create {
    #[bpaf(positional("DIR"))]
    dir: String,
    #[bpaf(positional("NAME"), some("a name is required"))]
    name: Vec<String>,
}
impl Verb<Ctx> for Create {
    fn name(&self) -> &'static str {
        "create"
    }
    fn run(self: Box<Self>, _ctx: &Ctx) -> Result<(), Exit> {
        Ok(())
    }
}
fn create_cmd() -> impl Parser<BoxedVerb<Ctx>> {
    create().map(|c| Box::new(c) as BoxedVerb<Ctx>)
}

#[derive(Debug, Clone, Bpaf)]
struct List {}
impl Verb<Ctx> for List {
    fn name(&self) -> &'static str {
        "list"
    }
    fn run(self: Box<Self>, _ctx: &Ctx) -> Result<(), Exit> {
        Ok(())
    }
}
fn list_cmd() -> impl Parser<BoxedVerb<Ctx>> {
    list().map(|c| Box::new(c) as BoxedVerb<Ctx>)
}

#[derive(Debug, Clone, Bpaf)]
struct Status {}
impl Verb<Ctx> for Status {
    fn name(&self) -> &'static str {
        "status"
    }
    fn run(self: Box<Self>, _ctx: &Ctx) -> Result<(), Exit> {
        Ok(())
    }
}
fn status_cmd() -> impl Parser<BoxedVerb<Ctx>> {
    status().map(|c| Box::new(c) as BoxedVerb<Ctx>)
}

fn app() -> App<Globals, Ctx> {
    let event = Group::new()
        .leaf("create", create_cmd())
        .leaf("list", list_cmd());
    App::new(globals())
        .group("event", event)
        .leaf("status", status_cmd())
}

// --- L1: a leaf parser parses positionals/options correctly ------------------

#[test]
fn l1_leaf_parses_positionals() {
    let parsed = create()
        .to_options()
        .run_inner(&["some/dir", "My", "Event"][..])
        .expect("parses");
    assert_eq!(parsed.dir, "some/dir");
    assert_eq!(parsed.name, vec!["My", "Event"]);
}

// --- cli! macro: raw tokens -> argv ------------------------------------------

#[test]
fn cli_macro_maps_tokens_to_argv() {
    assert_eq!(
        cli! { event create x "My Event" },
        vec!["event", "create", "x", "My Event"]
    );
}

#[test]
fn cli_macro_handles_flags() {
    assert_eq!(
        cli! { --home x -v status },
        vec!["--home", "x", "-v", "status"]
    );
    // A value containing a comma must be quoted (Rust lexer limitation).
    assert_eq!(
        cli! { --share "email,phone" },
        vec!["--share", "email,phone"]
    );
}

#[test]
fn cli_macro_empty_is_empty() {
    let empty: Vec<String> = cli! {};
    assert!(empty.is_empty());
}

// --- L2 + suggestions + exit codes: verb_test! reads like the terminal -------

verb_test! {
    app: app();

    // L2: dispatch selects the right verb.
    create_ok: { event create x "My Event" } => |r| {
        let (_g, verb) = r.expect("parses");
        assert_eq!(verb.name(), "create");
    },
    list_ok: { event list } => |r| {
        let (_g, verb) = r.expect("parses");
        assert_eq!(verb.name(), "list");
    },
    status_ok: { status } => |r| {
        let (_g, verb) = r.expect("parses");
        assert_eq!(verb.name(), "status");
    },
    globals_then_verb: { --home somewhere status } => |r| {
        let (_g, verb) = r.expect("parses");
        assert_eq!(verb.name(), "status");
    },

    // L2: a missing positional is a usage error (exit 2) naming the arg.
    missing_name_usage: { event create x } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
        // `some("a name is required")` supplies the missing-arg message.
        assert!(e.unwrap_stderr().contains("a name is required"));
    },

    // exit-code remap: --help is success (0).
    help_is_zero: { --help } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 0);
    },
    group_help_is_zero: { event --help } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 0);
    },

    // suggestion golden matrix: stderr names the intended target.
    typo_top_level_command: { stauts } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
        assert!(e.unwrap_stderr().contains("status"));
    },
    typo_nested_command: { event creat x y } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
        assert!(e.unwrap_stderr().contains("create"));
    },
    typo_long_flag: { --hom x status } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
        assert!(e.unwrap_stderr().contains("home"));
    },
    short_long_confusion: { -home x status } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
        assert!(e.unwrap_stderr().contains("home"));
    },
}
