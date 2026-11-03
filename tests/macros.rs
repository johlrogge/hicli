//! `commands!` and `app!` as a consumer writes them.
//!
//! The list macros, on their own: what a bare path means, what an expression
//! means, and that `app!` builds exactly the tree the builder chain does.
//! Groups have their own file — `tests/groups.rs` — because `#[group]` is about
//! *discovery*, and everything here is about a list that is written out.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{pure, Bpaf};
use hicli::{
    app, cli, cli_test, command, commands, external, App, Arg, Command, ExternalCommand, Failure,
    Outcome,
};

// --- the consumer's context and argument structs ------------------------------

/// The shared context. It records what ran, so a run-level test can assert
/// *that* a command ran.
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

/// A one-positional argument struct; its bpaf constructor is `title()`, and
/// `#[derive(Arg)]` is what makes it a `#[command]` parameter.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Title(#[bpaf(positional("TITLE"))] String);

// --- the commands -------------------------------------------------------------

/// create an event
#[command]
pub fn create(t: Title, ctx: &Ctx) -> Outcome {
    ctx.note(format!("create:{}", t.0));
    Ok(())
}

/// show the roster
#[command]
pub fn roster(ctx: &Ctx) -> Outcome {
    ctx.note("roster");
    Ok(())
}

/// The commands of a plain, unannotated module — registered by path, one at a
/// time. A module is not a group unless it says so.
pub mod audience {
    use super::*;

    /// define an audience
    #[command]
    pub fn define(t: Title, ctx: &Ctx) -> Outcome {
        ctx.note(format!("define:{}", t.0));
        Ok(())
    }

    /// remove an audience
    #[command]
    pub fn remove(ctx: &Ctx) -> Outcome {
        ctx.note("remove");
        Ok(())
    }
}

// --- the app ------------------------------------------------------------------

/// A never-executed external subcommand, registered by manifest.
fn ext() -> ExternalCommand {
    ExternalCommand::new("foo", "an external subcommand", "/nonexistent/hicli-foo")
}

/// The whole tree, declared.
fn app_by_macro() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [create, roster, audience::define, audience::remove, external(ext())],
    }
}

/// The same tree, written out by hand with the primitives the macros expand to
/// — including the command handles, called by their generated names.
fn app_by_hand() -> App<(), Ctx> {
    App::new(pure(())).commands(vec![
        __hicli_create(),
        __hicli_roster(),
        audience::__hicli_define(),
        audience::__hicli_remove(),
        external(ext()),
    ])
}

// --- what the list means ------------------------------------------------------

cli_test! {
    app: app_by_macro();

    // A bare ident names the handle beside the item of that name.
    a_bare_name_is_a_registration: { create "Summer Party" } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.path(), ["create"]);
    },
    // … and a path names it one module along, with no `use` to bring it here.
    a_path_names_a_handle_one_module_along: { define Friends } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.path(), ["define"]);
    },
    // An external is a command like any other — and is not executed to find out.
    an_external_is_registered: { foo --anything } => |r| {
        let (_g, invocation) = r.expect("parses");
        assert_eq!(invocation.path(), ["foo"]);
    },
}

#[test]
fn a_declared_command_runs() {
    let log = log();
    let code = app_by_macro().run_with_args(cli! { create "Summer Party" }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["create:Summer Party"]);
}

// --- the macros are sugar over the primitives ---------------------------------

#[test]
fn commands_mixes_functions_and_externals() {
    let cs: Vec<Command<Ctx>> = commands![create, audience::define, external(ext())];
    let names: Vec<&str> = cs.iter().map(Command::name).collect();
    assert_eq!(names, ["create", "define", "foo"]);
}

#[test]
fn commands_accepts_a_trailing_comma() {
    let cs: Vec<Command<Ctx>> = commands![create, roster,];
    assert_eq!(cs.len(), 2);
}

#[test]
fn a_local_command_is_passed_as_an_expression() {
    // A bare `c` would name a handle called `__hicli_c`; `(c)` is the
    // documented escape for a local that already holds a `Command`.
    let c: Command<Ctx> = hicli::command("apply", "apply everything", pure(()), |(), _: &Ctx| {});
    let cs: Vec<Command<Ctx>> = commands![(c)];
    assert_eq!(cs.iter().map(Command::name).collect::<Vec<_>>(), ["apply"]);
}

#[test]
fn app_builds_the_same_tree_as_the_builder_chain() {
    for line in [
        cli! { create "Summer Party" },
        cli! { roster },
        cli! { define Friends },
    ] {
        let (_g, by_macro) = app_by_macro().parse(line.clone()).expect("parses");
        let (_g, by_hand) = app_by_hand().parse(line.clone()).expect("parses");
        assert_eq!(by_macro.path(), by_hand.path(), "for `{line:?}`");
    }
}

#[test]
fn an_empty_list_is_an_empty_vec() {
    let cs: Vec<Command<Ctx>> = commands![];
    assert!(cs.is_empty());
}
