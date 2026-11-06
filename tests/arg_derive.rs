//! End-to-end tests for `#[derive(Arg)]`: the impl it writes, and the
//! completion it wires up.
//!
//! Two commands take the same `Id`, and neither says anything about ids: one is
//! built with `hicli::command(…)` **by hand**, taking `<Id as Arg>::parser()`
//! as its parser — the primitive form a command that outgrows the macro is
//! written in — and one is an ordinary `#[command]`, which reaches the same impl
//! through the same trait. That they behave identically is the claim.

use hicli::bpaf::{pure, Bpaf, Parser};
use hicli::{app, cli, command, App, Arg, Candidate};

/// The shared context: these tests never run a command, only parse and
/// complete.
pub struct Ctx;

/// An event id — a newtype whose completion comes from the type, not from the
/// command that takes it.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_ids)]
pub struct Id(#[bpaf(positional("ID"))] String);

/// The ids on offer, filtered by what has been typed so far.
///
/// The filtering is deliberate: `bpaf` passes a completer's output through
/// unchanged, so a completer that does not filter offers everything always.
fn complete_ids(typed: &Id) -> Vec<Candidate> {
    ["e-17", "e-23", "x-1"]
        .iter()
        .filter(|c| c.starts_with(typed.0.as_str()))
        .map(|c| ((*c).to_string(), Some("an event".to_string())))
        .collect()
}

/// A home directory — completed by the shell itself.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]
pub struct Home(#[bpaf(positional("DIR"))] String);

/// A multi-field argument struct: no completion, so the derive writes the same
/// impl it writes for a newtype.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct ShareOpts {
    #[bpaf(long, argument("WHO"))]
    share: String,
    #[bpaf(long, switch)]
    quiet: bool,
}

/// One command taking an `Id`, so the type's completion is reachable from the
/// command line.
fn app() -> App<(), Ctx> {
    App::new(pure(())).command(hicli::command(
        "show",
        "show an event",
        <Id as Arg>::parser(),
        |_id: Id, _ctx: &Ctx| {},
    ))
}

/// open an event
///
/// The `#[command]` form of the command above: the parameter is the type, and
/// `#[command]` resolves it through `Arg` — the same impl `show` names by hand.
#[command]
pub fn open(id: Id, ctx: &Ctx) {
    let _ = (id, ctx);
}

/// The same `Id`, reached through the macro.
fn app_by_macro() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [open] }
}

#[test]
fn the_derived_impl_parses_through_the_bpaf_constructor() {
    let id = <Id as Arg>::parser()
        .to_options()
        .run_inner(&["e-17"][..])
        .expect("parses");
    assert_eq!(id.0, "e-17");
}

#[test]
fn completion_comes_from_the_argument_type() {
    // Nothing about `show` mentions ids: the candidates travel with `Id`, which
    // is the whole point of putting the completer on the type.
    let got = app().complete(cli! { show "e-" });
    assert!(got.contains(&"e-17".to_string()), "{got:?}");
    assert!(got.contains(&"e-23".to_string()), "{got:?}");
    // … and the completer filtered, because bpaf did not.
    assert!(!got.contains(&"x-1".to_string()), "{got:?}");
}

#[test]
fn a_command_reaches_the_same_impl_through_its_parameter_type() {
    // Nothing at `open` mentions `Arg`, a constructor or a completer: the
    // parameter's *type* is the whole declaration.
    let (_g, invocation) = app_by_macro().parse(cli! { open "e-17" }).expect("parses");
    assert_eq!(invocation.path(), ["open"]);

    let got = app_by_macro().complete(cli! { open "e-" });
    assert!(got.contains(&"e-17".to_string()), "{got:?}");
    assert!(got.contains(&"e-23".to_string()), "{got:?}");
    assert!(!got.contains(&"x-1".to_string()), "{got:?}");
}

#[test]
fn a_shell_completion_leaves_the_parser_alone() {
    // `complete = dirs` is an instruction to the shell — there is nothing for
    // `App::complete` to report — so what is asserted is that attaching it did
    // not change what the type parses.
    let home = <Home as Arg>::parser()
        .to_options()
        .run_inner(&["src"][..])
        .expect("parses");
    assert_eq!(home.0, "src");
}

/// Sharing
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Sharing {
    /// who to share with
    #[bpaf(long, argument("WHO"))]
    #[allow(dead_code)] // only the help is under test
    share: Option<String>,
}

/// share an event
#[command]
pub fn share(s: Sharing, id: Id, ctx: &Ctx) {
    let _ = (s, id, ctx);
}

#[test]
fn bpaf_mode_help_is_unchanged() {
    // A regression guard for spelled mode being additive: a bpaf-mode type's
    // help is bpaf's — its struct doc a group heading, its field docs the
    // rows — exactly as before #8 step 2.
    let a: App<(), Ctx> = app! { globals: pure(()), commands: [share] };
    let out = a
        .parse(cli! { share --help })
        .err()
        .expect("help")
        .unwrap_stdout();
    assert_eq!(
        out,
        "share an event\n\
         \n\
         Usage: share [--share=WHO] ID\n\
         \n\
         Sharing\n\
         \x20       --share=WHO  who to share with\n\
         \n\
         An event id — a newtype whose completion comes from the type, not from the\n\
         \x20 command that takes it.\n\
         \n\
         Available options:\n\
         \x20   -h, --help       Prints help information\n"
    );
}

#[test]
fn a_multi_field_struct_parses_its_options() {
    let opts = <ShareOpts as Arg>::parser()
        .to_options()
        .run_inner(&["--share", "x"][..])
        .expect("parses");
    assert_eq!(opts.share, "x");
    assert!(!opts.quiet);
}
