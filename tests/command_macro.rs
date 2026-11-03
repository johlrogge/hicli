//! `#[command]` as a consumer sees it: through `hicli::command`, with argument
//! types derived locally by `#[derive(Bpaf, Arg)]` — and one, `Note`, whose
//! `Arg` impl is written out by hand, because the macro asks the *type* how it
//! parses and cannot tell the two apart.
//!
//! Every command below is written the way the documentation tells a consumer to
//! write one — a documented free function — and is then exercised both ways the
//! macro leaves it usable: as the function it still is (`create(r, t, &ctx)`)
//! and through the handle it registers by name (`commands: [create]`).

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{positional, pure, Bpaf, Parser};
use hicli::{
    app, cli, cli_test, command, exit_code_for, App, Arg, Command, Failure, Outcome, Success,
};

// --- the consumer's context and argument structs ------------------------------

/// The shared context. It records what ran, so a run-level test can assert
/// *that* a command ran and *with which arguments* — `run_with_args` returns
/// only an exit code.
///
/// `pub`, like the argument structs below, because `create` is a `pub` command
/// and its handle — `pub`, always — returns a `Command<Ctx>`. That is the
/// ordinary Rust rule about a public signature, not something `#[command]`
/// adds.
pub struct Ctx {
    log: Rc<RefCell<Vec<String>>>,
}

impl Ctx {
    /// Record one line of evidence.
    fn note(&self, line: impl Into<String>) {
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

/// A reusable argument group — two positionals written once.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct EventRef {
    #[bpaf(positional("DIR"))]
    dir: String,
    #[bpaf(positional("INSTANCE"))]
    instance: String,
}

/// A newtype argument struct: its bpaf constructor is `title()`, and
/// `#[derive(Arg)]` is the one line that says so.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Title(#[bpaf(positional("TITLE"))] String);

/// A switch, to prove a non-positional argument struct composes the same way.
#[derive(Debug, Clone, Bpaf, Arg)]
struct Flags {
    #[bpaf(long, switch)]
    seal: bool,
}

/// An argument type with **no derive at all**: `Arg` is written out by hand,
/// which is the escape hatch for anything `#[derive(Bpaf)]` cannot spell. A
/// `#[command]` cannot tell the difference — it asks the type, not the derive.
#[derive(Debug, Clone)]
pub struct Note(String);

impl Arg for Note {
    fn parser() -> impl Parser<Self> {
        positional::<String>("NOTE").map(Note)
    }
}

// --- the commands -------------------------------------------------------------

/// create an event
///
/// The event directory is created if it does not exist.
#[command]
pub fn create(r: EventRef, t: Title, ctx: &Ctx) -> Outcome {
    ctx.note(format!("create:{}:{}:{}", r.dir, r.instance, t.0));
    Ok(())
}

/// create an event twice
///
/// One command's body calling another by its declared name — `create` is still
/// the function it was written as, so this is an ordinary call, with no `::run`
/// and no handle in sight.
#[command]
fn create_twice(r: EventRef, t: Title, ctx: &Ctx) -> Outcome {
    create(r.clone(), t.clone(), ctx)?;
    create(r, t, ctx)
}

/// show status
///
/// Arity 0 with a context, and a private `fn`: the function stays private, the
/// handle beside it is `pub` like every handle.
#[command]
fn status(ctx: &Ctx) -> Outcome {
    ctx.note("status");
    Ok(())
}

/// name one thing
#[command]
fn single(t: Title, ctx: &Ctx) {
    ctx.note(format!("single:{}", t.0));
}

/// apply everything
///
/// Three argument structs. The switch is declared *first* because bpaf
/// requires every positional to sit at the right-hand end of the structure it
/// is composed into — a rule about the parameter order, not the typed order:
/// `--seal` is still accepted anywhere on the command line.
#[command]
fn apply(f: Flags, r: EventRef, t: Title, ctx: &Ctx) -> Outcome<Success> {
    ctx.note(format!("apply:{}:{}:{}:{}", r.dir, r.instance, t.0, f.seal));
    Ok(Success::code(7))
}

/// take the context first
#[command]
fn ctx_first(ctx: &Ctx, t: Title) -> Success {
    ctx.note(format!("ctx-first:{}", t.0));
    Success::code(3)
}

/// add an administrator
#[command]
fn add_admin(t: Title, ctx: &Ctx) -> Outcome {
    ctx.note(format!("add-admin:{}", t.0));
    Ok(())
}

/// remove an administrator
#[command(name = "rm")]
fn remove_admin(ctx: &Ctx) {
    ctx.note("rm");
}

/// fail at runtime
#[command]
fn boom(ctx: &Ctx) -> Outcome {
    ctx.note("boom");
    Err(Failure::runtime("boom"))
}

/// print the version
///
/// No `&T` parameter, so the handle is generic over `Ctx` and works in any app.
#[command]
fn version() {
    // Deliberately no context: this is the generic-handle case.
}

/// annotate a thing
///
/// Its one argument implements `Arg` by hand, with no `#[derive(Bpaf)]` and no
/// `#[derive(Arg)]` anywhere.
#[command]
fn annotate(n: Note, ctx: &Ctx) {
    ctx.note(format!("annotate:{}", n.0));
}

// --- the app ------------------------------------------------------------------

/// Every command registered by its bare name: `commands!` rewrites each one to
/// the hidden handle `#[command]` wrote beside it.
fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [
            create,
            create_twice,
            status,
            single,
            apply,
            ctx_first,
            add_admin,
            remove_admin,
            boom,
            version,
            annotate,
        ],
    }
}

// --- names --------------------------------------------------------------------

cli_test! {
    app: app();

    // The ident is the name.
    create_is_selected: { create "./ev" main "Summer Party" } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.path(), ["create"]);
    },
    // `_` becomes `-`.
    add_admin_is_dashed: { "add-admin" "Jo" } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.name(), "add-admin");
    },
    // … and the underscored spelling is not a command.
    the_underscored_spelling_is_not_a_command: { add_admin "Jo" } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
    },
    // `#[command(name = "rm")]` wins over the ident.
    the_name_attribute_overrides_the_ident: { rm } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.name(), "rm");
    },
    remove_admin_is_not_a_command: { "remove-admin" } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
    },
    // Arity 0 with a context.
    status_takes_no_arguments: { status } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.name(), "status");
    },
    // Arity 1: `construct!(title(),)`, not a one-tuple.
    a_single_argument_struct_parses: { single "Summer Party" } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.name(), "single");
    },
    // The generic handle, registered under this app's concrete `Ctx`.
    a_context_free_command_is_registered: { version } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.path(), ["version"]);
    },
    // The arguments are still the argument structs' own parsers, with their own
    // usage errors.
    a_missing_positional_is_a_usage_error: { create "./ev" main } => |r| {
        let e = r.err().expect("expected a parse failure");
        assert_eq!(exit_code_for(&e), 2);
        assert!(e.unwrap_stderr().contains("TITLE"));
    },
}

// --- help ---------------------------------------------------------------------

#[test]
fn the_first_doc_line_is_the_command_help() {
    let e = app()
        .parse(cli! { --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    let out = e.unwrap_stdout();
    for word in [
        "create",
        "create an event",
        "add-admin",
        "add an administrator",
        "rm",
        "remove an administrator",
    ] {
        assert!(out.contains(word), "help missing `{word}`:\n{out}");
    }
}

#[test]
fn the_rest_of_the_doc_comment_becomes_details() {
    let e = app()
        .parse(cli! { create --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    let out = e.unwrap_stdout();
    // First line: the description (the heading of `create --help`).
    assert!(
        out.contains("create an event"),
        "command help missing its description:\n{out}"
    );
    // The rest: `.details(…)`, rendered as the header.
    assert!(
        out.contains("The event directory is created if it does not exist."),
        "command help missing its details:\n{out}"
    );
}

#[test]
fn a_single_line_doc_comment_has_no_details() {
    let e = app()
        .parse(cli! { single --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    // The help line appears once, as the description — there is no `.details`
    // call to repeat it as a header.
    assert_eq!(
        out.matches("name one thing").count(),
        1,
        "a one-line doc comment should not also become details:\n{out}"
    );
}

// --- arguments ----------------------------------------------------------------

#[test]
fn positionals_are_consumed_in_declaration_order() {
    let log = log();
    let code = app().run_with_args(cli! { create "./ev" main "Summer Party" }, ctx_from(&log));
    assert_eq!(code, 0);
    // DIR, INSTANCE, then TITLE — the order the parameters are declared in, not
    // the order bpaf happened to build them.
    assert_eq!(ran(&log), ["create:./ev:main:Summer Party"]);
}

#[test]
fn a_single_argument_struct_is_not_a_tuple() {
    // Arity 1 is the trailing-comma case: `construct!(title())` would yield a
    // parser-of-parser and fail to compile as a `Title`.
    let log = log();
    let code = app().run_with_args(cli! { single "Summer Party" }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["single:Summer Party"]);
}

#[test]
fn three_argument_structs_compose() {
    let log = log();
    let code = app().run_with_args(
        cli! { apply "./ev" main "Summer Party" --seal },
        ctx_from(&log),
    );
    assert_eq!(code, 7);
    assert_eq!(ran(&log), ["apply:./ev:main:Summer Party:true"]);
}

#[test]
fn a_hand_written_arg_is_an_argument_type_like_any_other() {
    // `Note` has no derives: its parser is an `impl Arg` written out by hand.
    // `#[command]` resolves every parameter through the trait, so the command
    // registers and parses exactly as the derived ones do.
    let log = log();
    let code = app().run_with_args(cli! { annotate "a note" }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["annotate:a note"]);

    // … and the metavar it named reaches the command line's usage error.
    let e = app()
        .parse(cli! { annotate })
        .err()
        .expect("expected a parse failure");
    assert_eq!(exit_code_for(&e), 2);
    assert!(e.unwrap_stderr().contains("NOTE"));
}

#[test]
fn the_context_may_be_declared_first() {
    let log = log();
    let code = app().run_with_args(cli! { "ctx-first" "Summer Party" }, ctx_from(&log));
    assert_eq!(code, 3);
    assert_eq!(ran(&log), ["ctx-first:Summer Party"]);
}

// --- return types: every `IntoOutcome` impl, through the macro ----------------

#[test]
fn a_unit_return_exits_zero() {
    let log = log();
    assert_eq!(app().run_with_args(cli! { single X }, ctx_from(&log)), 0);
    assert_eq!(ran(&log), ["single:X"]);
}

#[test]
fn a_success_return_carries_its_code() {
    let log = log();
    assert_eq!(
        app().run_with_args(cli! { "ctx-first" X }, ctx_from(&log)),
        3
    );
}

#[test]
fn an_outcome_failure_is_one() {
    let log = log();
    assert_eq!(app().run_with_args(cli! { boom }, ctx_from(&log)), 1);
    assert_eq!(ran(&log), ["boom"]);
}

#[test]
fn an_outcome_of_success_carries_its_code() {
    let log = log();
    let code = app().run_with_args(cli! { apply "./ev" main X }, ctx_from(&log));
    assert_eq!(code, 7);
}

// --- the function, and the handle beside it -----------------------------------

/// An `EventRef` naming the fixture event.
fn an_event() -> EventRef {
    EventRef {
        dir: "./ev".into(),
        instance: "main".into(),
    }
}

#[test]
fn the_function_is_untouched() {
    // `create` is the function that was written: same name, same parameters,
    // same return type, called with plain values and no parser anywhere.
    let log = log();
    let ctx = Ctx {
        log: Rc::clone(&log),
    };
    let outcome = create(an_event(), Title("Summer Party".into()), &ctx);
    assert!(outcome.is_ok());
    assert_eq!(ran(&log), ["create:./ev:main:Summer Party"]);
}

#[test]
fn a_private_command_is_callable_within_its_module() {
    // `status` is a private `fn` and stays one — callable from where it was
    // written, which is this file.
    let log = log();
    let ctx = Ctx {
        log: Rc::clone(&log),
    };
    status(&ctx).expect("ok");
    assert_eq!(ran(&log), ["status"]);
}

#[test]
fn a_command_calls_a_command() {
    // Through the run path, so the evidence is what the *called* function
    // recorded: `create_twice` calls `create` twice, by name.
    let log = log();
    let code = app().run_with_args(
        cli! { "create-twice" "./ev" main "Summer Party" },
        ctx_from(&log),
    );
    assert_eq!(code, 0);
    assert_eq!(
        ran(&log),
        [
            "create:./ev:main:Summer Party",
            "create:./ev:main:Summer Party"
        ]
    );
}

#[test]
fn the_handle_is_callable() {
    // The handle is an ordinary function returning a `Command`, named by the
    // documented convention — so it can be called by hand, or handed to
    // `App::command`, as well as registered by its command's bare name.
    let c: Command<Ctx> = __hicli_create();
    assert_eq!(c.name(), "create");

    // … and, being a `FnOnce() -> Command`, it is also an `IntoCommand`.
    let app: App<(), Ctx> = App::new(pure(())).command(__hicli_status);
    let (_g, invocation) = app.parse(cli! { status }).expect("parses");
    assert_eq!(invocation.name(), "status");
}

#[test]
fn a_command_without_a_context_is_generic_over_ctx() {
    // No `&T` parameter, so the handle names no context type: it builds a
    // `Command<Ctx>` for *any* `Ctx`, here one this test file never uses.
    let c: Command<String> = __hicli_version();
    assert_eq!(c.name(), "version");
}
