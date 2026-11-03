//! Command and app metadata (ADR-008): `alias`, `hide`, the examples footer,
//! and the app's own `about` / `version` — driven through the macros the way a
//! consumer writes them, and asserted in-process.

use hicli::bpaf::pure;
use hicli::{app, cli, command, exit_code_for, group, App, Failure};

/// The shared context: nothing is recorded, only parsing and help are tested.
pub struct Ctx;

/// show the status
#[command(alias = "st")]
fn status(_ctx: &Ctx) {}

/// do the thing nobody is told about
#[command(hide)]
fn secret(_ctx: &Ctx) {}

/// list the library
///
/// # Examples
///
/// ```sh
/// demo list
/// demo ls --long
/// ```
#[command(alias = "ls", alias = "l")]
fn list(_ctx: &Ctx) {}

/// experimental commands
#[group(hide, alias = "e")]
mod experimental {
    use super::*;

    /// try something
    #[command]
    pub fn attempt(_ctx: &Ctx) {}
}

/// administer the library
///
/// # Examples
///
/// ```sh
/// demo admin audit
/// ```
#[group]
mod admin {
    use super::*;

    /// check the library for problems
    #[command]
    pub fn audit(_ctx: &Ctx) {}

    /// remove everything, irreversibly
    #[command(hide)]
    pub fn purge(_ctx: &Ctx) {}
}

/// The tree, with an about and a version on the composition root.
fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [status, secret, list, experimental, admin],
    }
    .about("A demo of command metadata.")
    .version("9.8.7")
}

/// `app <args…>`'s stdout, from a `ParseFailure` that exits `0`.
fn stdout_of(args: Vec<String>) -> String {
    let e = app()
        .parse(args)
        .err()
        .expect("short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    e.unwrap_stdout()
}

/// The path `args` selects.
fn path_of(args: Vec<String>) -> Vec<&'static str> {
    let (_g, invocation) = app().parse(args).expect("parses");
    invocation.path().to_vec()
}

/// An `mk_ctx` that always succeeds.
fn mk_ctx(_: ()) -> Result<Ctx, Failure> {
    Ok(Ctx)
}

// --- about and version --------------------------------------------------------

#[test]
fn about_is_printed_above_usage_in_top_level_help() {
    let out = stdout_of(cli! { --help });
    let about = out.find("A demo of command metadata.").expect(&out);
    let usage = out.find("Usage:").expect(&out);
    assert!(about < usage, "about is not above Usage:\n{out}");
}

#[test]
fn version_prints_the_version_to_stdout() {
    assert_eq!(stdout_of(cli! { --version }), "Version: 9.8.7\n");
    assert_eq!(stdout_of(cli! { -V }), "Version: 9.8.7\n");
}

#[test]
fn version_exits_zero_on_the_run_path() {
    assert_eq!(app().run_with_args(cli! { --version }, mk_ctx), 0);
}

#[test]
fn version_is_listed_in_top_level_help() {
    let out = stdout_of(cli! { --help });
    assert!(out.contains("-V, --version"), "{out}");
}

// --- hide -----------------------------------------------------------------------

#[test]
fn a_hidden_command_is_not_in_help_or_completion() {
    let out = stdout_of(cli! { --help });
    assert!(out.contains("status"), "{out}");
    assert!(
        !out.contains("secret"),
        "a hidden command is listed:\n{out}"
    );
    assert!(!app().complete(cli! { "" }).contains(&"secret".to_string()));
    assert!(app().complete(cli! { "sec" }).is_empty());
}

#[test]
fn a_hidden_command_still_parses_and_runs() {
    assert_eq!(path_of(cli! { secret }), ["secret"]);
    assert_eq!(app().run_with_args(cli! { secret }, mk_ctx), 0);
}

#[test]
fn a_hidden_group_is_unlisted_but_reachable_by_name_and_alias() {
    let out = stdout_of(cli! { --help });
    assert!(!out.contains("experimental"), "{out}");
    assert_eq!(
        path_of(cli! { experimental attempt }),
        ["experimental", "attempt"]
    );
    assert_eq!(path_of(cli! { e attempt }), ["experimental", "attempt"]);
}

// --- alias ----------------------------------------------------------------------

#[test]
fn an_alias_dispatches_with_the_canonical_path() {
    assert_eq!(path_of(cli! { st }), ["status"]);
    assert_eq!(path_of(cli! { ls }), ["list"]);
    assert_eq!(path_of(cli! { l }), ["list"]);
    assert_eq!(app().run_with_args(cli! { st }, mk_ctx), 0);
}

#[test]
fn an_alias_is_not_listed_in_help() {
    let out = stdout_of(cli! { --help });
    let commands = out
        .split_once("Available commands:")
        .expect("a command list")
        .1;
    let names: Vec<&str> = commands
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(names, ["status", "list", "admin"], "{out}");
}

// --- bpaf's own suggestions -------------------------------------------------------

/// The usage error `app <args…>` returns from `App::parse` — `bpaf`'s own
/// message, with its own suggestions and without hicli's run-path hint.
fn usage_error_of(args: Vec<String>) -> String {
    let e = app().parse(args).err().expect("a usage error");
    assert_eq!(exit_code_for(&e), 2);
    e.unwrap_stderr()
}

#[test]
fn bpaf_never_suggests_a_hidden_top_level_command() {
    let err = usage_error_of(cli! { secrt });
    assert!(
        !err.contains("secret"),
        "a hidden name is suggested:\n{err}"
    );
}

#[test]
fn bpaf_never_suggests_a_hidden_child_of_a_listed_group() {
    let err = usage_error_of(cli! { admin purg });
    assert!(!err.contains("purge"), "a hidden name is suggested:\n{err}");
    // The hidden child still parses when typed exactly.
    assert_eq!(path_of(cli! { admin purge }), ["admin", "purge"]);
}

// --- the examples footer ----------------------------------------------------------

#[test]
fn a_commands_examples_are_its_help_footer() {
    let out = stdout_of(cli! { list --help });
    assert!(
        out.find("Available options").expect(&out) < out.find("Examples:").expect(&out),
        "{out}"
    );
    assert!(
        out.ends_with("Examples:\n  demo list\n  demo ls --long\n"),
        "{out}"
    );
}

#[test]
fn a_groups_examples_are_its_help_footer() {
    let out = stdout_of(cli! { admin --help });
    assert!(
        out.find("Available commands").expect(&out) < out.find("Examples:").expect(&out),
        "{out}"
    );
    assert!(out.ends_with("Examples:\n  demo admin audit\n"), "{out}");
    assert!(!out.contains("purge"), "a hidden child is listed:\n{out}");
}

#[test]
fn the_examples_footer_is_shown_through_an_alias_too() {
    let out = stdout_of(cli! { ls --help });
    assert!(out.ends_with("demo ls --long\n"), "{out}");
}
