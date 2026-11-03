//! End-to-end tests for `hicli`, written the way the user types.
//!
//! The consumer app mirrors `examples/demo.rs`: globals with `--home`, an
//! `event` group with `create`/`list` commands, and a top-level `status`
//! command — declared in the **macro form** a consumer is told to use
//! (`#[command]`, `#[group]`, `app!`).
//!
//! Two fixtures are deliberately left in the hand-written primitive form —
//! `app_with_composed_args` (one `hicli::command(…)`) and
//! `app_with_two_creates` (one `hicli::group(…)`) — because that is exactly
//! what the macros expand to, and it should keep working for anyone whose
//! command outgrows `#[command]`.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{construct, pure, Bpaf, Parser};
use hicli::{
    app, cli, cli_test, command, commands, exit_code_for, group, App, Arg, ExternalCommand,
    Failure, Outcome, Success,
};

/// The shared context. It carries the globals-derived `home` plus a log, so a
/// run-level test can assert *that* a command ran and *which* context it got —
/// `App::run_with_args` returns only an exit code, and stdout/stderr go to the
/// real streams.
///
/// `pub` because a `#[command]`'s handle is `pub` and returns a `Command<Ctx>`.
pub struct Ctx {
    home: String,
    log: Rc<RefCell<Vec<String>>>,
}

impl Ctx {
    /// Record that `command` ran, together with the globals-derived `home`.
    fn ran(&self, command: &'static str) {
        self.note(format!("{command}:{}", self.home));
    }

    /// Record an arbitrary line — used where the *arguments* are the evidence.
    fn note(&self, line: String) {
        self.log.borrow_mut().push(line);
    }
}

/// A fresh recording log.
fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

/// An `mk_ctx` that derives the context from the globals and records into `log`.
fn ctx_from(log: &Rc<RefCell<Vec<String>>>) -> impl FnOnce(Globals) -> Result<Ctx, Failure> {
    let log = Rc::clone(log);
    move |g: Globals| Ok(Ctx { home: g.home, log })
}

/// What ran, in order — `["status:somewhere"]`, or empty if nothing ran.
fn ran(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    log.borrow().clone()
}

/// Global options, accepted before or after the subcommand.
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
    home: String,
}

/// The arguments of `event create`: `<DIR> <NAME…>`.
///
/// Named `CreateArgs`, not `Create`, because `#[derive(Bpaf)]` names its
/// constructor after the type: a struct called `Create` would generate
/// `fn create()` and collide with the `create` command's handle.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct CreateArgs {
    #[bpaf(positional("DIR"))]
    dir: String,
    #[bpaf(positional("NAME"), some("a name is required"))]
    name: Vec<String>,
}

// --- the consumer's tree, in the macro form ----------------------------------

/// manage events
#[group]
mod event {
    use super::*;

    /// create an event
    ///
    /// The directory is created if missing.
    #[command]
    pub fn create(_args: CreateArgs, ctx: &Ctx) {
        ctx.ran("create");
    }

    /// list events
    #[command]
    pub fn list(ctx: &Ctx) {
        ctx.ran("list");
    }
}

/// show demo status
#[command]
fn status(ctx: &Ctx) {
    ctx.ran("status");
}

fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        // `event` is the module above: `#[group]` wrote its handle here, beside
        // the module, so the group is registered by the module's own name.
        commands: [event, status],
    }
}

// --- one command per outcome, for the run-level tests ------------------------
//
// Names are deliberately far (> 2 edits) from `event`/`status`, so the
// suggestion registry behaves the same as it does for `app()`.

/// finish with code 7
#[command]
fn code7(ctx: &Ctx) -> Success {
    ctx.ran("code7");
    Success::code(7)
}

/// finish with code 0
#[command]
fn code0(ctx: &Ctx) -> Success {
    ctx.ran("code0");
    Success::code(0)
}

/// fail at runtime
#[command]
fn boom(ctx: &Ctx) -> Outcome {
    ctx.ran("boom");
    Err(Failure::runtime("boom"))
}

/// fail with a usage error
#[command]
fn misuse(ctx: &Ctx) -> Outcome {
    ctx.ran("misuse");
    Err(Failure::usage("misused"))
}

/// `app()` plus one command per outcome.
fn app_with_outcomes() -> App<Globals, Ctx> {
    app().commands(commands![code7, code0, boom, misuse])
}

// --- L1: a command's parser parses positionals/options correctly -------------

#[test]
fn l1_command_parses_positionals() {
    // An argument struct is an ordinary bpaf parser, tested once and trusted by
    // every command that takes it — which is the point of making arguments
    // types.
    let parsed = create_args()
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
const EXT_BIN: &str = "/nonexistent/hicli-external-foo";

fn app_with_external() -> App<Globals, Ctx> {
    app().command(hicli::external(ExternalCommand::new(
        "foo",
        "the foo plugin",
        EXT_BIN,
    )))
}

// --- externals: registered, advertised, dispatched — never spawned -----------

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
    let (_g, invocation) = app_with_external().parse(cli! { foo a b }).expect("parses");
    assert_eq!(invocation.name(), "foo");
    // An external is mounted like any other top-level command, so it has the
    // same kind of path.
    assert_eq!(invocation.path(), ["foo"]);
    // The invocation is dropped here without `run` — the only call that would
    // spawn anything.
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
    let (_g, invocation) = app_with_external().parse(cli! { foo a b }).expect("parses");
    assert_eq!(invocation.name(), "foo");
}

#[test]
fn external_does_not_shadow_registered_commands() {
    let (_g, invocation) = app_with_external().parse(cli! { status }).expect("parses");
    assert_eq!(invocation.name(), "status");
}

// --- completion: the registry feeds the surface bpaf completes from ----------

/// Dynamic completion is driven in-process, through `App::complete` — the test
/// seam that hides the wire protocol, exactly as `parse` hides `run_inner`.
#[test]
fn completion_lists_commands_and_the_external() {
    let got = app_with_external().complete(cli! { "" });
    for word in ["event", "status", "foo"] {
        assert!(
            got.iter().any(|c| c == word),
            "completion missing `{word}`:\n{got:?}"
        );
    }
}

/// The same request, one layer down: `--bpaf-complete-rev=1` is the revision
/// bpaf 0.9.28 emits in its generated shell scripts, and the result
/// short-circuits as `ParseFailure::Completion`, whose payload
/// `unwrap_stdout()` returns. Kept raw so the *wire format* stays covered — if
/// bpaf ever bumps the revision, this is the test that says so rather than one
/// that merely finds no candidates.
///
/// Revision **2** is not an option here: asked for through `run_inner` it exits
/// the process, which would take the test runner with it.
#[test]
fn completion_speaks_bpafs_revision_1_protocol() {
    let e = app_with_external()
        .into_parser()
        .run_inner(&["--bpaf-complete-rev=1", ""][..])
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
    // `manage events` is the `#[group]` module's doc comment; `show demo
    // status` is the command's — neither is written twice anywhere.
    for word in ["event", "manage events", "status", "show demo status"] {
        assert!(out.contains(word), "help missing `{word}`:\n{out}");
    }
}

#[test]
fn group_help_lists_command_help() {
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
fn command_help_shows_description() {
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
        "command help missing its description:\n{out}"
    );
}

#[test]
fn command_help_shows_details() {
    let e = app()
        .parse(cli! { event create --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    // The rest of the doc comment becomes `.details(…)`, rendered as the header
    // under the one-line description.
    assert!(
        out.contains("The directory is created if missing."),
        "command help missing its details:\n{out}"
    );
}

// --- L2 + suggestions + exit codes: cli_test! reads like the terminal --------

cli_test! {
    app: app();

    // L2: dispatch selects the right command.
    create_ok: { event create x "My Event" } => |r| {
        let (_g, invocation) =r.expect("parses");
        assert_eq!(invocation.name(), "create");
        // The path is baked in when the tree is folded, so it says which
        // `create` this is — not just that a `create` matched.
        assert_eq!(invocation.path(), ["event", "create"]);
    },
    list_ok: { event list } => |r| {
        let (_g, invocation) =r.expect("parses");
        assert_eq!(invocation.name(), "list");
        assert_eq!(invocation.path(), ["event", "list"]);
    },
    status_ok: { status } => |r| {
        let (_g, invocation) =r.expect("parses");
        assert_eq!(invocation.name(), "status");
        // A top-level command's path is just itself.
        assert_eq!(invocation.path(), ["status"]);
    },
    globals_then_command: { --home somewhere status } => |r| {
        let (_g, invocation) =r.expect("parses");
        assert_eq!(invocation.name(), "status");
    },

    // Globals are composed with the command at the top level, so they are
    // accepted after the subcommand too — not only before it.
    globals_after_command: { status --home somewhere } => |r| {
        let (_g, invocation) =r.expect("parses");
        assert_eq!(invocation.name(), "status");
    },
    globals_after_group_command: { event list --home somewhere } => |r| {
        let (_g, invocation) =r.expect("parses");
        assert_eq!(invocation.name(), "list");
    },
    // Even a command with free-form positionals (`some("NAME")`) does not
    // swallow the trailing global: bpaf's positional parsers stop at a dashed
    // token, so `--home somewhere` still reaches the globals.
    globals_after_free_positionals: { event create dir Name --home somewhere } => |r| {
        let (g, invocation) =r.expect("parses");
        assert_eq!(invocation.name(), "create");
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
    // suggestions; hicli's gap-closer is unit-tested in `src/app.rs` and
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

// --- paths: which command, not just which name -------------------------------

/// Two commands named `create`, in two different groups. Only `path()` can
/// tell them apart — which is why `Invocation` carries one.
///
/// Written with `hicli::group(…)` and `hicli::command(…)` **by hand**: this is
/// the primitive form `#[group]` and `#[command]` expand to, kept covered so a
/// consumer whose tree outgrows the macros still has a supported way down.
fn app_with_two_creates() -> App<Globals, Ctx> {
    App::new(globals()).commands(vec![
        hicli::group("event", "manage events", commands![event::create]),
        hicli::group(
            "id",
            "manage ids",
            vec![hicli::command(
                "create",
                "create an id",
                pure(()),
                |(), ctx: &Ctx| ctx.ran("create"),
            )],
        ),
    ])
}

#[test]
fn same_name_in_two_groups_is_distinguished_by_path() {
    let (_g, event_create) = app_with_two_creates()
        .parse(cli! { event create dir Name })
        .expect("parses");
    let (_g, id_create) = app_with_two_creates()
        .parse(cli! { id create })
        .expect("parses");

    // Same name …
    assert_eq!(event_create.name(), id_create.name());
    // … different command.
    assert_eq!(event_create.path(), ["event", "create"]);
    assert_eq!(id_create.path(), ["id", "create"]);
}

// --- return shapes: every `IntoOutcome` impl is reachable from a command -----

/// One command per legal return type. The *code* is the evidence: `()` -> 0,
/// `Success` -> its code, `Outcome` -> 0 or 1, `Outcome<Success>` -> its
/// code or 2.
mod returns {
    use super::*;

    /// return ()
    #[command]
    pub fn ret_unit(ctx: &Ctx) {
        ctx.ran("ret-unit");
    }

    /// return Success
    #[command]
    pub fn ret_success(ctx: &Ctx) -> Success {
        ctx.ran("ret-success");
        Success::code(7)
    }

    /// return Outcome
    #[command]
    pub fn ret_outcome(ctx: &Ctx) -> Outcome {
        ctx.ran("ret-outcome");
        Err(Failure::runtime("boom"))
    }

    /// return Outcome<Success>
    #[command]
    pub fn ret_outcome_success(ctx: &Ctx) -> Outcome<Success> {
        ctx.ran("ret-outcome-success");
        Ok(Success::code(3))
    }
}

fn app_with_returns() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [
            returns::ret_unit,
            returns::ret_success,
            returns::ret_outcome,
            returns::ret_outcome_success,
        ],
    }
}

#[test]
fn command_accepts_a_unit_return() {
    let log = log();
    // A dashed command name must be quoted in `cli!` — bare `ret-unit` lexes as
    // three tokens and the muncher would read `-unit` as a short flag.
    let code = app_with_returns().run_with_args(cli! { "ret-unit" }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["ret-unit:."]);
}

#[test]
fn command_accepts_a_success_return() {
    let log = log();
    let code = app_with_returns().run_with_args(cli! { "ret-success" }, ctx_from(&log));
    assert_eq!(code, 7);
    assert_eq!(ran(&log), ["ret-success:."]);
}

#[test]
fn command_accepts_an_outcome_return() {
    let log = log();
    let code = app_with_returns().run_with_args(cli! { "ret-outcome" }, ctx_from(&log));
    assert_eq!(code, 1);
    assert_eq!(ran(&log), ["ret-outcome:."]);
}

#[test]
fn command_accepts_an_outcome_of_success_return() {
    let log = log();
    let code = app_with_returns().run_with_args(cli! { "ret-outcome-success" }, ctx_from(&log));
    assert_eq!(code, 3);
    assert_eq!(ran(&log), ["ret-outcome-success:."]);
}

// --- composed argument structs: the primitive form `#[command]` expands to ---
//
// Neither type below derives `Arg`, deliberately. `hicli::command(…)` takes
// **any** `impl Parser<A>` and never mentions the trait, so this fixture is
// living proof that the primitive path stays open to a parser that no argument
// type speaks for — which is what a command that outgrows `#[command]` needs.

/// A reusable argument group — the pattern that makes `#[command]`'s owned
/// parameters worth having.
#[derive(Debug, Clone, Bpaf)]
pub struct EventRef {
    #[bpaf(positional("DIR"))]
    dir: String,
    #[bpaf(positional("INSTANCE"))]
    instance: String,
}

/// A newtype argument struct: its bpaf constructor is `title()`.
#[derive(Debug, Clone, Bpaf)]
pub struct Title(#[bpaf(positional("TITLE"))] String);

/// The `construct!` shape `#[command]` generates, spelled out by hand —
/// including the arity-1 trailing comma, which is load-bearing. This is the one
/// `hicli::command(…)` fixture kept in the primitive form, and its elements
/// are bare bpaf constructors: no `Arg`, no trait bound, any parser will do.
fn app_with_composed_args() -> App<Globals, Ctx> {
    App::new(globals()).commands(vec![
        hicli::command(
            "compose",
            "take two argument structs",
            // Two structs, composed the way `#[command]` composes a command's
            // owned parameters — except that `#[command]` reaches each through
            // `<T as Arg>::parser()`, and this reaches the constructor itself.
            // The trailing comma is load-bearing at arity 1 (below) and
            // harmless here.
            construct!(event_ref(), title(),),
            |(r, t): (EventRef, Title), ctx: &Ctx| {
                ctx.note(format!("compose:{}:{}:{}", r.dir, r.instance, t.0));
            },
        ),
        hicli::command(
            "single",
            "take one argument struct",
            // Arity 1 *needs* the trailing comma: `construct!(title())` yields a
            // parser-of-parser, not a `Title`.
            construct!(title(),),
            |t: Title, ctx: &Ctx| {
                ctx.note(format!("single:{}", t.0));
            },
        ),
    ])
}

#[test]
fn composed_arg_structs_consume_positionals_in_declaration_order() {
    let log = log();
    let code = app_with_composed_args()
        .run_with_args(cli! { compose "./ev" main "Summer Party" }, ctx_from(&log));
    assert_eq!(code, 0);
    // DIR, INSTANCE, then TITLE — the order the structs appear in `construct!`,
    // not the order bpaf happened to build them.
    assert_eq!(ran(&log), ["compose:./ev:main:Summer Party"]);
}

#[test]
fn a_single_argument_struct_is_not_a_tuple() {
    let log = log();
    let code =
        app_with_composed_args().run_with_args(cli! { single "Summer Party" }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["single:Summer Party"]);
}

// --- run level: the whole pipeline through `run_with_args` -------------------
//
// These drive `App::run`'s pipeline with an explicit argv. Only the returned
// exit code is assertable: help, usage errors and a `Failure`'s message go to
// the process's real stdout/stderr, which `run_with_args` does not capture. That
// noise is harmless — cargo captures per-test output unless a test fails. What
// the command *did* is observed through the recording `Ctx` instead.

#[test]
fn run_parses_builds_context_and_runs_the_command() {
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { --home somewhere status }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["status:somewhere"]);
}

#[test]
fn run_usage_error_is_two() {
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { event create x }, ctx_from(&log));
    assert_eq!(code, 2);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_help_is_zero() {
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { --help }, ctx_from(&log));
    assert_eq!(code, 0);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_typo_command_is_two() {
    // This is the gap-closer path; the appended `did you mean` line goes to the
    // real stderr and is not assertable here — `augment_stderr`/`suggest` are
    // unit-tested in src/app.rs.
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { stauts }, ctx_from(&log));
    assert_eq!(code, 2);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_unknown_command_is_two() {
    // Same path with no suggestion within 2 edits — proves the silent branch of
    // `augment_stderr` doesn't panic or change the code.
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { wildlydifferent }, ctx_from(&log));
    assert_eq!(code, 2);
    assert!(ran(&log).is_empty());
}

#[test]
fn run_mk_ctx_failure_skips_the_command() {
    let log = log();
    let rec = Rc::clone(&log);
    let mk_ctx = move |g: Globals| {
        rec.borrow_mut().push(format!("mk_ctx:{}", g.home));
        Err(Failure::runtime("no home"))
    };
    let code = app_with_outcomes().run_with_args(cli! { --home somewhere status }, mk_ctx);
    // Code 1 is unreachable from a parse failure (2) or from `status` (which
    // succeeds with 0), so this proves mk_ctx ran with the parsed globals, that
    // its `Failure` travelled the same report path as a command's, and that no
    // command ran after it — one would have appended `status:somewhere`.
    assert_eq!(code, 1);
    assert_eq!(ran(&log), ["mk_ctx:somewhere"]);
}

#[test]
fn run_propagates_a_commands_exit_code() {
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { code7 }, ctx_from(&log));
    assert_eq!(code, 7);
    // `--home` falls back to `.`.
    assert_eq!(ran(&log), ["code7:."]);
}

#[test]
fn run_command_success_code_zero_is_ok() {
    // A command finishing with an explicit `Success::code(0)` exits 0. Under
    // `Exit` this state could only be spelled on the failure side, where it had
    // to be normalised to 1.
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { code0 }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["code0:."]);
}

#[test]
fn run_command_runtime_error_is_one() {
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { boom }, ctx_from(&log));
    assert_eq!(code, 1);
    assert_eq!(ran(&log), ["boom:."]);
}

#[test]
fn run_command_usage_error_is_two() {
    let log = log();
    let code = app_with_outcomes().run_with_args(cli! { misuse }, ctx_from(&log));
    assert_eq!(code, 2);
    assert_eq!(ran(&log), ["misuse:."]);
}

#[test]
fn run_external_spawn_failure_is_one() {
    // `EXT_BIN` cannot exist, so `Command::status()` fails at exec: no child
    // process is ever created, so this stays inside the "no subprocess in
    // tests" rule (ADR-003 accepted the same argument for the unit test in
    // src/external.rs). What is new here is the wiring: the spawn failure
    // becomes `Failure::Runtime` and travels the full `run_with_args` pipeline
    // out as exit code 1.
    let log = log();
    let code = app_with_external().run_with_args(cli! { foo a b }, ctx_from(&log));
    // `code == 1` carries the proof: a successful parse with no run returns 0.
    assert_eq!(code, 1);
    // The external never records into `Ctx`, so this line is a
    // placeholder for symmetry with the other run-level tests, not evidence.
    assert!(ran(&log).is_empty());
}
