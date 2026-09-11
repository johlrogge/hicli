//! End-to-end tests for `verbandi`, written the way the user types.
//!
//! The consumer app mirrors `examples/demo.rs`: globals with `--home`, an
//! `event` group with `create`/`list` leaves, and a top-level `status` leaf.

use std::cell::RefCell;
use std::rc::Rc;

use verbandi::bpaf::{pure, Bpaf, Parser};
use verbandi::{cli, exit_code_for, verb_test, App, BoxedVerb, Exit, ExternalVerb, Group, Verb};

/// The shared context. It carries the globals-derived `home` plus a log, so a
/// run-level test can assert *that* a verb ran and *which* context it got —
/// `App::run_with_args` returns only an exit code, and stdout/stderr go to the
/// real streams.
struct Ctx {
    home: String,
    log: Rc<RefCell<Vec<String>>>,
}

impl Ctx {
    /// Record that `verb` ran, together with the globals-derived `home`.
    fn ran(&self, verb: &'static str) {
        self.log.borrow_mut().push(format!("{verb}:{}", self.home));
    }
}

/// A fresh recording log.
fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

/// An `mk_ctx` that derives the context from the globals and records into `log`.
fn ctx_from(log: &Rc<RefCell<Vec<String>>>) -> impl FnOnce(Globals) -> Result<Ctx, Exit> {
    let log = Rc::clone(log);
    move |g: Globals| Ok(Ctx { home: g.home, log })
}

/// What ran, in order — `["status:somewhere"]`, or empty if no verb ran.
fn ran(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    log.borrow().clone()
}

#[derive(Debug, Clone, Bpaf)]
struct Globals {
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
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
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> {
        ctx.ran(self.name());
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
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> {
        ctx.ran(self.name());
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
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> {
        ctx.ran(self.name());
        Ok(())
    }
}
fn status_cmd() -> impl Parser<BoxedVerb<Ctx>> {
    status().map(|c| Box::new(c) as BoxedVerb<Ctx>)
}

fn app() -> App<Globals, Ctx> {
    let event = Group::new()
        .leaf("create", "create an event", create_cmd())
        .leaf("list", "list events", list_cmd());
    App::new(globals())
        .group("event", "manage events", event)
        .leaf("status", "show demo status", status_cmd())
}

/// A verb that records that it ran and then fails with a fixed `Exit`.
struct Failing {
    name: &'static str,
    exit: Exit,
}
impl Verb<Ctx> for Failing {
    fn name(&self) -> &'static str {
        self.name
    }
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> {
        ctx.ran(self.name);
        Err(self.exit)
    }
}
fn failing_cmd(name: &'static str, exit: Exit) -> impl Parser<BoxedVerb<Ctx>> {
    pure(()).map(move |()| {
        Box::new(Failing {
            name,
            exit: exit.clone(),
        }) as BoxedVerb<Ctx>
    })
}

/// `app()` plus one leaf per failure mode, for the run-level tests. Names are
/// deliberately far (> 2 edits) from `event`/`status`, so the suggestion
/// registry behaves the same as it does for `app()`.
fn app_with_exits() -> App<Globals, Ctx> {
    app()
        .leaf(
            "code7",
            "fail with code 7",
            failing_cmd("code7", Exit::Code(7)),
        )
        .leaf(
            "code0",
            "fail with code 0",
            failing_cmd("code0", Exit::Code(0)),
        )
        .leaf(
            "boom",
            "fail at runtime",
            failing_cmd("boom", Exit::runtime("boom")),
        )
        .leaf(
            "misuse",
            "fail with a usage error",
            failing_cmd("misuse", Exit::Usage("misused".into())),
        )
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

/// Deliberately a path that cannot exist: if registration, help rendering or
/// dispatch ever spawned the binary, every test below would fail.
const EXT_BIN: &str = "/nonexistent/verbandi-external-foo";

fn app_with_external() -> App<Globals, Ctx> {
    app().external(ExternalVerb::new("foo", "the foo plugin", EXT_BIN))
}

// --- external verbs: registered, advertised, dispatched — never spawned ------

#[test]
fn external_is_listed_in_top_level_help() {
    let e = app_with_external()
        .parse(cli! { --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    let out = e.unwrap_stdout();
    for word in ["foo", "the foo plugin"] {
        assert!(out.contains(word), "help missing `{word}`:\n{out}");
    }
}

#[test]
fn external_is_selected_by_name() {
    let (_g, verb) = app_with_external().parse(cli! { foo a b }).expect("parses");
    assert_eq!(verb.name(), "foo");
    // The verb is dropped here without `run` — the only call that would spawn
    // anything.
}

#[test]
fn external_help_does_not_execute_the_binary() {
    assert!(
        !std::path::Path::new(EXT_BIN).exists(),
        "test seam broken: {EXT_BIN} exists"
    );
    let e = app_with_external()
        .parse(cli! { --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    let (_g, verb) = app_with_external().parse(cli! { foo a b }).expect("parses");
    assert_eq!(verb.name(), "foo");
}

#[test]
fn external_does_not_shadow_registered_commands() {
    let (_g, verb) = app_with_external().parse(cli! { status }).expect("parses");
    assert_eq!(verb.name(), "status");
}

// --- completion: the registry feeds the surface bpaf completes from ----------

/// Dynamic completion is driven in-process through bpaf's own wire protocol:
/// `--bpaf-complete-rev=1` is the revision bpaf 0.9.27 emits in its generated
/// shell scripts. The result short-circuits as `ParseFailure::Completion`,
/// whose payload `unwrap_stdout()` returns.
#[test]
fn completion_lists_commands_and_the_external() {
    let r = app_with_external()
        .into_parser()
        .run_inner(&["--bpaf-complete-rev=1", ""][..]);
    let e = r
        .err()
        .expect("completion short-circuits with a ParseFailure");
    assert_eq!(
        exit_code_for(&e),
        0,
        "bpaf may have bumped the completion protocol revision; \
         `--bpaf-complete-rev=1` was parsed as an unknown flag"
    );
    let out = e.unwrap_stdout();
    for word in ["event", "status", "foo"] {
        assert!(out.contains(word), "completion missing `{word}`:\n{out}");
    }
}

// --- help text: every command carries a one-line description -----------------

#[test]
fn top_level_help_lists_commands_with_help() {
    let e = app()
        .parse(cli! { --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    let out = e.unwrap_stdout();
    for word in ["event", "manage events", "status", "show demo status"] {
        assert!(out.contains(word), "help missing `{word}`:\n{out}");
    }
}

#[test]
fn group_help_lists_leaf_help() {
    let e = app()
        .parse(cli! { event --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    let out = e.unwrap_stdout();
    for word in ["create", "create an event", "list", "list events"] {
        assert!(out.contains(word), "group help missing `{word}`:\n{out}");
    }
}

#[test]
fn leaf_help_shows_description() {
    let e = app()
        .parse(cli! { event create --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    let out = e.unwrap_stdout();
    // The description is the heading of `event create --help` — this proves
    // `descr`, not just the parent's one-line `help`.
    assert!(
        out.contains("create an event"),
        "leaf help missing its description:\n{out}"
    );
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

    // Globals are composed with the command at the top level, so they are
    // accepted after the subcommand too — not only before it.
    globals_after_verb: { status --home somewhere } => |r| {
        let (_g, verb) = r.expect("parses");
        assert_eq!(verb.name(), "status");
    },
    globals_after_group_leaf: { event list --home somewhere } => |r| {
        let (_g, verb) = r.expect("parses");
        assert_eq!(verb.name(), "list");
    },
    // Even a leaf with free-form positionals (`some("NAME")`) does not swallow
    // the trailing global: bpaf's positional parsers stop at a dashed token, so
    // `--home somewhere` still reaches the globals.
    globals_after_free_positionals: { event create dir Name --home somewhere } => |r| {
        let (g, verb) = r.expect("parses");
        assert_eq!(verb.name(), "create");
        assert_eq!(g.home, "somewhere");
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
    // These cases route through `App::parse`, so they observe bpaf's own
    // suggestions; verbandi's gap-closer is unit-tested in `src/app.rs` and
    // only on the run path (`App::run` / `App::run_with_args`).
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

// --- run level: the whole pipeline through `run_with_args` -------------------
//
// These drive `App::run`'s pipeline with an explicit argv. Only the returned
// exit code is assertable: help, usage errors and an `Exit`'s message go to the
// process's real stdout/stderr, which `run_with_args` does not capture. That
// noise is harmless — cargo captures per-test output unless a test fails. What
// the verb *did* is observed through the recording `Ctx` instead.

#[test]
fn run_parses_builds_context_and_runs_the_verb() {
    let log = log();
    let code = app_with_exits().run_with_args(cli! { --home somewhere status }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["status:somewhere"]);
}

#[test]
fn run_usage_error_is_two() {
    let log = log();
    let code = app_with_exits().run_with_args(cli! { event create x }, ctx_from(&log));
    assert_eq!(code, 2);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_help_is_zero() {
    let log = log();
    let code = app_with_exits().run_with_args(cli! { --help }, ctx_from(&log));
    assert_eq!(code, 0);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_typo_command_is_two() {
    // This is the gap-closer path; the appended `did you mean` line goes to the
    // real stderr and is not assertable here — `augment_stderr`/`suggest` are
    // unit-tested in src/app.rs.
    let log = log();
    let code = app_with_exits().run_with_args(cli! { stauts }, ctx_from(&log));
    assert_eq!(code, 2);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_unknown_command_is_two() {
    // Same path with no suggestion within 2 edits — proves the silent branch of
    // `augment_stderr` doesn't panic or change the code.
    let log = log();
    let code = app_with_exits().run_with_args(cli! { wildlydifferent }, ctx_from(&log));
    assert_eq!(code, 2);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_mk_ctx_failure_skips_the_verb() {
    let log = log();
    let rec = Rc::clone(&log);
    let mk_ctx = move |g: Globals| {
        rec.borrow_mut().push(format!("mk_ctx:{}", g.home));
        Err(Exit::Code(9))
    };
    let code = app_with_exits().run_with_args(cli! { --home somewhere status }, mk_ctx);
    // Code 9 is unreachable from a parse failure (2) or a verb (none ran), so
    // this proves mk_ctx ran with the parsed globals, that its `Exit` travelled
    // the same report path as a verb's, and that no verb ran after it — a verb
    // would have appended `status:somewhere`.
    assert_eq!(code, 9);
    assert_eq!(ran(&log), ["mk_ctx:somewhere"]);
}

#[test]
fn run_propagates_a_verbs_exit_code() {
    let log = log();
    let code = app_with_exits().run_with_args(cli! { code7 }, ctx_from(&log));
    assert_eq!(code, 7);
    // `--home` falls back to `.`.
    assert_eq!(ran(&log), ["code7:."]);
}

#[test]
fn run_verb_exit_code_zero_is_not_success() {
    let log = log();
    let code = app_with_exits().run_with_args(cli! { code0 }, ctx_from(&log));
    assert_eq!(code, 1);
    assert_eq!(ran(&log), ["code0:."]);
}

#[test]
fn run_verb_runtime_error_is_one() {
    let log = log();
    let code = app_with_exits().run_with_args(cli! { boom }, ctx_from(&log));
    assert_eq!(code, 1);
    assert_eq!(ran(&log), ["boom:."]);
}

#[test]
fn run_verb_usage_error_is_two() {
    let log = log();
    let code = app_with_exits().run_with_args(cli! { misuse }, ctx_from(&log));
    assert_eq!(code, 2);
    assert_eq!(ran(&log), ["misuse:."]);
}

#[test]
fn run_external_spawn_failure_is_one() {
    // `EXT_BIN` cannot exist, so `Command::status()` fails at exec: no child
    // process is ever created, so this stays inside the "no subprocess in
    // tests" rule (ADR-003 accepted the same argument for the unit test in
    // src/external.rs). What is new here is the wiring: the spawn failure
    // becomes `Exit::Runtime` and travels the full `run_with_args` pipeline
    // out as exit code 1.
    let log = log();
    let code = app_with_external().run_with_args(cli! { foo a b }, ctx_from(&log));
    // `code == 1` carries the proof: a successful parse with no run returns 0.
    assert_eq!(code, 1);
    // `ExternalRun` never records into `Ctx`, so this line is a placeholder for
    // symmetry with the other run-level tests, not evidence.
    assert!(ran(&log).is_empty());
}
