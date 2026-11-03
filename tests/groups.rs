//! `#[group]` as a consumer writes it.
//!
//! The tree below is declared the way the documentation tells a consumer to
//! declare one — `#[command]` functions inside `#[group]` modules, one `app!`
//! at the top — and is then exercised through the ordinary parse seam. There is
//! no list of children anywhere: a child is a child because it is in the
//! module.
//!
//! Note the single `use hicli::group;` at the top. It imports the attribute
//! *and* the function of the same name, and this file uses both: the tree is
//! declared with `#[group]` and then rebuilt with `group(…)` by hand, so the
//! two can be compared.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{pure, Bpaf};
use hicli::{
    app, cli, command, commands, group, App, Arg, Command, ExternalCommand, Failure, Outcome,
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

// --- the tree -----------------------------------------------------------------

/// events and instances
///
/// An event is a directory; an instance is a branch of it.
#[group]
pub mod event {
    use super::*;

    /// create an event
    #[command]
    pub fn create(t: Title, ctx: &Ctx) -> Outcome {
        ctx.note(format!("create:{}", t.0));
        Ok(())
    }

    /// manage audiences
    ///
    /// An audience is a set of people invited together.
    #[group]
    pub mod audience {
        use super::*;

        /// define an audience
        #[command]
        pub fn define(t: Title, ctx: &Ctx) -> Outcome {
            ctx.note(format!("define:{}", t.0));
            Ok(())
        }

        /// the people in an audience
        #[group]
        pub mod member {
            use super::*;

            /// add a member
            #[command]
            pub fn add(t: Title, ctx: &Ctx) -> Outcome {
                ctx.note(format!("add:{}", t.0));
                Ok(())
            }
        }
    }

    // `any()` is never true, so this command is not compiled — and its
    // registration, which carries the same `#[cfg]`, is not compiled either.
    /// show the roster
    #[cfg(any())]
    #[command]
    pub fn roster(ctx: &Ctx) -> Outcome {
        ctx.note("roster");
        Ok(())
    }

    // Not a `#[group]`, so not a child — which is the whole reason the rule is
    // "marked", not "every module". Every consumer has one of these.
    #[cfg(test)]
    mod tests {
        #[test]
        fn a_test_module_is_ordinary_rust() {
            assert_eq!(2 + 2, 4);
        }
    }
}

/// manage administrators
#[group]
mod add_admin {
    use super::*;

    /// grant a role
    #[command]
    pub fn grant(ctx: &Ctx) -> Outcome {
        ctx.note("grant");
        Ok(())
    }
}

// Nothing in here takes a context, so the group's handle is generic over `Ctx`
// and this group mounts in any app.
/// the odds and ends
#[group]
pub mod tools {
    use super::*;

    /// print the version
    #[command]
    pub fn version() {}
}

// --- a group the macro cannot express -----------------------------------------

/// A never-executed external subcommand, registered by manifest.
fn ext() -> ExternalCommand {
    ExternalCommand::new("foo", "an external subcommand", "/nonexistent/hicli-foo")
}

/// `hicli` registration handle for the hand-written `plugins` group.
///
/// An external has no item to mark, so it cannot be a *discovered* child. The
/// way out is to write the handle by the documented convention — which
/// `commands![plugins]` then finds, exactly as if `#[group]` had written it.
#[doc(hidden)]
pub fn __hicli_plugins() -> Command<Ctx> {
    group(
        "plugins",
        "the installed plugins",
        commands![hicli::external(ext())],
    )
}

// --- the app ------------------------------------------------------------------

/// The whole tree, declared.
fn app_by_macro() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [event, add_admin, tools, plugins],
    }
}

/// The same tree, written out by hand with the primitives `#[group]` expands
/// to — including the command handles, called by their generated names.
fn app_by_hand() -> App<(), Ctx> {
    App::new(pure(())).commands(vec![
        group(
            "event",
            "events and instances",
            vec![
                event::__hicli_create(),
                group(
                    "audience",
                    "manage audiences",
                    vec![
                        event::audience::__hicli_define(),
                        group(
                            "member",
                            "the people in an audience",
                            vec![event::audience::member::__hicli_add()],
                        ),
                    ],
                )
                .details("An audience is a set of people invited together."),
            ],
        )
        .details("An event is a directory; an instance is a branch of it."),
        group(
            "add-admin",
            "manage administrators",
            vec![add_admin::__hicli_grant()],
        ),
        group("tools", "the odds and ends", vec![tools::__hicli_version()]),
        __hicli_plugins(),
    ])
}

// --- membership ---------------------------------------------------------------

#[test]
fn a_function_in_the_module_is_a_command_in_the_group() {
    let (_g, invocation) = app_by_macro()
        .parse(cli! { event create "Summer Party" })
        .expect("parses");
    assert_eq!(invocation.path(), ["event", "create"]);
}

#[test]
fn a_group_nests_three_deep() {
    // Three `#[group]` modules and a command: the handle of each reaches exactly
    // one level down, and the path is assembled as the tree is folded.
    let (_g, invocation) = app_by_macro()
        .parse(cli! { event audience member add Jo })
        .expect("parses");
    assert_eq!(invocation.path(), ["event", "audience", "member", "add"]);
}

#[test]
fn a_group_ident_is_dashed() {
    let (_g, invocation) = app_by_macro()
        .parse(cli! { "add-admin" grant })
        .expect("parses");
    assert_eq!(invocation.path(), ["add-admin", "grant"]);
}

#[test]
fn the_underscored_group_spelling_is_not_a_command() {
    let e = app_by_macro()
        .parse(cli! { add_admin grant })
        .err()
        .expect("expected a parse failure");
    assert_eq!(hicli::exit_code_for(&e), 2);
}

#[test]
fn a_declared_command_runs() {
    let log = log();
    let code = app_by_macro().run_with_args(cli! { event audience member add Jo }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["add:Jo"]);
}

#[test]
fn a_hand_written_handle_is_found_by_name() {
    // `commands![plugins]` resolved to `__hicli_plugins()`, and the external
    // inside it is a command like any other — never executed to find out.
    let (_g, invocation) = app_by_macro()
        .parse(cli! { plugins foo a b })
        .expect("parses");
    assert_eq!(invocation.path(), ["plugins", "foo"]);
}

// --- what is *not* a child ----------------------------------------------------

#[test]
fn a_configured_out_command_is_not_registered() {
    // `#[cfg(any())]` rode from the function onto its own `push`, so the
    // registration went with the function.
    let e = app_by_macro()
        .parse(cli! { event roster })
        .err()
        .expect("`roster` is not compiled, so it is not a command");
    assert_eq!(hicli::exit_code_for(&e), 2);

    let e = app_by_macro()
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
fn an_unmarked_module_is_not_a_child() {
    // `mod tests` is in `mod event`, and is not in the `event` group.
    let e = app_by_macro()
        .parse(cli! { event --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    assert!(
        !out.contains("tests"),
        "an unmarked module is listed:\n{out}"
    );
}

// --- help ---------------------------------------------------------------------

#[test]
fn a_groups_first_doc_line_is_its_help() {
    let e = app_by_macro()
        .parse(cli! { --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    for word in [
        "event",
        "events and instances",
        "add-admin",
        "manage administrators",
    ] {
        assert!(out.contains(word), "help missing `{word}`:\n{out}");
    }
}

#[test]
fn the_rest_of_a_groups_doc_comment_becomes_details() {
    let e = app_by_macro()
        .parse(cli! { event audience --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    assert!(
        out.contains("manage audiences"),
        "group help missing its description:\n{out}"
    );
    assert!(
        out.contains("An audience is a set of people invited together."),
        "group help missing its details:\n{out}"
    );
}

#[test]
fn a_single_line_doc_comment_has_no_details() {
    let e = app_by_macro()
        .parse(cli! { "add-admin" --help })
        .err()
        .expect("--help short-circuits with a ParseFailure");
    let out = e.unwrap_stdout();
    assert_eq!(
        out.matches("manage administrators").count(),
        1,
        "a one-line doc comment should not also become details:\n{out}"
    );
}

// --- the context type ---------------------------------------------------------

#[test]
fn a_context_free_group_mounts_in_any_app() {
    // Nothing in `tools` names a context, so its handle is generic — and the
    // same group is mounted twice under two unrelated `Ctx` types.
    let here: App<(), Ctx> = app! { globals: pure(()), commands: [tools] };
    let (_g, invocation) = here.parse(cli! { tools version }).expect("parses");
    assert_eq!(invocation.path(), ["tools", "version"]);

    let elsewhere: App<(), String> = app! { globals: pure(()), commands: [tools] };
    let (_g, invocation) = elsewhere.parse(cli! { tools version }).expect("parses");
    assert_eq!(invocation.path(), ["tools", "version"]);
}

// --- the macros are sugar over the primitives ---------------------------------

#[test]
fn a_group_handle_is_an_ordinary_function() {
    // `pub fn __hicli_event() -> Command<Ctx>`: callable, nameable, and
    // usable anywhere a `Command` is — the same shape `#[command]` writes,
    // which is why a list cannot tell a group from a command.
    let c: Command<Ctx> = __hicli_event();
    assert_eq!(c.name(), "event");
}

#[test]
fn the_macro_builds_the_same_tree_as_the_primitives() {
    for line in [
        cli! { event create "Summer Party" },
        cli! { event audience define Friends },
        cli! { event audience member add Jo },
        cli! { "add-admin" grant },
    ] {
        let (_g, by_macro) = app_by_macro().parse(line.clone()).expect("parses");
        let (_g, by_hand) = app_by_hand().parse(line.clone()).expect("parses");
        assert_eq!(by_macro.path(), by_hand.path(), "for `{line:?}`");
    }
}

#[test]
#[should_panic(expected = "has no commands")]
fn a_hand_built_empty_group_still_panics_when_the_tree_is_folded() {
    // The macro refuses an empty group at compile time; the runtime check stays
    // for a tree built by hand, which is the only way to reach this state.
    let empty: Command<Ctx> = group("empty", "a group nobody can enter", vec![]);
    let _ = App::new(pure(())).command(empty).into_parser();
}
