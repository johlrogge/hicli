//! Regression suite for johlrogge/hicli#9: an examples section written in a
//! `#[command]`'s doc comment renders as a footer, verbatim, after the options.
//!
//! The three spellings an author reaches for — a terminal-style `Examples:`
//! with an indented body, rustdoc's `# Examples` with an indented code block,
//! and `# Examples` with a ```` ```sh ```` fence — all land in the same place
//! with the same layout, in plain `--help` and in `--help --help` (ADR-008).
//! Assertions are on ordering and the footer's own lines, never on the whole
//! output.

use hicli::bpaf::Bpaf;
use hicli::{app, command, exit_code_for, App, Arg};

/// The (empty) shared context: only help is under test, nothing runs.
pub struct Ctx;

/// Global options — present because `app!` needs some.
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
    #[allow(dead_code)] // only the help is under test
    home: String,
}

// The search's one option, so the help has an options list to sit before the
// footer. Deliberately not a doc comment on the struct: bpaf would render it
// as a group heading above `--artist`.
#[allow(missing_docs)]
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Query {
    /// only this artist
    #[bpaf(long, argument("NAME"))]
    #[allow(dead_code)] // only the help is under test
    artist: Option<String>,
}

/// Search the library.
///
/// Searches by artist.
///
/// Examples:
///     mdma search --artist Bowie
///     mdma search --added=-7 --genre jazz
#[command]
fn indented(_q: Query, _ctx: &Ctx) {}

/// Search the library.
///
/// Searches by artist.
///
/// # Examples
///
///     mdma search --artist Bowie
///     mdma search --added=-7 --genre jazz
#[command]
fn heading(_q: Query, _ctx: &Ctx) {}

/// Search the library.
///
/// Searches by artist.
///
/// # Examples
///
/// ```sh
/// mdma search --artist Bowie
/// mdma search --added=-7 --genre jazz
/// ```
#[command]
fn fenced(_q: Query, _ctx: &Ctx) {}

fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [indented, heading, fenced],
    }
}

/// `app <args…>`'s help output.
fn help_of(args: &[&str]) -> String {
    let e = app()
        .parse(args.iter().copied())
        .err()
        .expect("--help short-circuits with a ParseFailure");
    assert_eq!(exit_code_for(&e), 0);
    e.unwrap_stdout()
}

/// What every spelling's footer renders as: the heading, then each example
/// on its own line, indented by two, at the very end of the help.
const FOOTER: &str =
    "Examples:\n  mdma search --artist Bowie\n  mdma search --added=-7 --genre jazz\n";

/// Assert the footer contract on one command's help, rendered with `extra`
/// (`[]` for plain `--help`, `["--help"]` for `--help --help`).
fn assert_footer(cmd: &str, extra: &[&str]) {
    let mut args = vec![cmd, "--help"];
    args.extend_from_slice(extra);
    let out = help_of(&args);

    let options = out
        .find("Available options")
        .unwrap_or_else(|| panic!("no options list:\n{out}"));
    let examples = out
        .find("Examples:")
        .unwrap_or_else(|| panic!("no examples:\n{out}"));
    assert!(options < examples, "examples before the options:\n{out}");
    assert!(out.ends_with(FOOTER), "footer not verbatim:\n{out}");

    // The details are the prose only: no example text leaked above the
    // options, and the details themselves are still there.
    let above = &out[..options];
    assert!(above.contains("Searches by artist."), "{out}");
    assert!(
        !above.contains("mdma"),
        "example text in the details:\n{out}"
    );
    assert!(
        !above.contains("Examples"),
        "heading in the details:\n{out}"
    );
    assert!(!out.contains("```"), "a fence was rendered:\n{out}");
}

#[test]
fn indented_examples_render_verbatim_after_the_options() {
    assert_footer("indented", &[]);
}

#[test]
fn heading_examples_render_verbatim_after_the_options() {
    assert_footer("heading", &[]);
}

#[test]
fn fenced_examples_render_verbatim_after_the_options() {
    assert_footer("fenced", &[]);
}

#[test]
fn indented_examples_survive_detailed_help() {
    assert_footer("indented", &["--help"]);
}

#[test]
fn heading_examples_survive_detailed_help() {
    assert_footer("heading", &["--help"]);
}

#[test]
fn fenced_examples_survive_detailed_help() {
    assert_footer("fenced", &["--help"]);
}

#[test]
fn examples_are_not_in_the_parents_listing() {
    // The footer belongs to the command's own help; the top-level list shows
    // the one-line help and nothing else.
    let out = help_of(&["--help"]);
    assert!(out.contains("Search the library."), "{out}");
    assert!(!out.contains("mdma"), "{out}");
}
