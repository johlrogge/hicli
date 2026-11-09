//! `# Arguments` (issue #23): a command's doc comment can replace the help row
//! of a parameter whose type is spelled by `#[arg(…)]`, through `ArgHelp`.
//!
//! The same `SortOpt` is taken by `list`, which overrides its help, and by
//! `search`, which does not; everything but that one row is identical.

use hicli::bpaf::{pure, Parser};
use hicli::{app, cli, command, exit_code_for, App, Arg, ArgHelp, Choice};

/// The shared context: nothing here runs a command.
pub struct Ctx;

/// The field a listing is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum SortField {
    /// Beats per minute
    Bpm,
    /// Track title
    Title,
}

/// Sort the listing by this field
#[derive(Debug, Clone, Arg)]
#[arg(long = "sort", short = 's', metavar = "FIELD")]
pub struct SortOpt(pub Option<SortField>);

/// What to group by
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice, Arg)]
#[arg(positional = "KEY")]
pub enum GroupKey {
    /// Artist name
    Artist,
    /// Album title
    Album,
}

/// List tracks
///
/// Prints one track per line.
///
/// # Arguments
///
/// * `by` - Order for this listing
#[command]
pub fn list(by: SortOpt, ctx: &Ctx) {
    let _ = (by, ctx);
}

/// Search tracks
#[command]
pub fn search(by: SortOpt, ctx: &Ctx) {
    let _ = (by, ctx);
}

/// Group tracks
///
/// # Arguments
///
/// - `key` - The field that
///   names each group
#[command]
pub fn group(key: GroupKey, ctx: &Ctx) {
    let _ = (key, ctx);
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [list, search, group] }
}

/// `app <args…>`'s help, which must exit 0.
fn help_of(args: Vec<String>) -> String {
    let e = app().parse(args).err().expect("--help short-circuits");
    assert_eq!(exit_code_for(&e), 0);
    e.unwrap_stdout()
}

/// `app <args…>`'s usage error, which must exit 2.
fn error_of(args: Vec<String>) -> String {
    let e = app().parse(args).err().expect("a usage error");
    assert_eq!(exit_code_for(&e), 2);
    e.unwrap_stderr()
}

#[test]
fn an_override_replaces_the_row_and_the_section_is_not_details() {
    assert_eq!(
        help_of(cli! { list --help }),
        "List tracks\n\
         \n\
         Usage: list [-s=FIELD]\n\
         \n\
         Prints one track per line.\n\
         \n\
         Available options:\n\
         \x20   -s, --sort=FIELD  Order for this listing [bpm, title]\n\
         \x20   -h, --help        Prints help information\n"
    );
}

#[test]
fn without_an_entry_the_row_is_the_types_help() {
    assert_eq!(
        help_of(cli! { search --help }),
        "Search tracks\n\
         \n\
         Usage: search [-s=FIELD]\n\
         \n\
         Available options:\n\
         \x20   -s, --sort=FIELD  Sort the listing by this field [bpm, title]\n\
         \x20   -h, --help        Prints help information\n"
    );
}

#[test]
fn a_choice_enum_parameter_can_be_overridden_too() {
    assert_eq!(
        help_of(cli! { group --help }),
        "Group tracks\n\
         \n\
         Usage: group KEY\n\
         \n\
         Available positional items:\n\
         \x20   KEY         The field that names each group [artist, album]\n\
         \n\
         Available options:\n\
         \x20   -h, --help  Prints help information\n"
    );
}

#[test]
fn completion_and_errors_are_the_same_with_or_without_an_override() {
    for cmd in ["list", "search"] {
        assert_eq!(
            app().complete(vec![cmd.to_string(), "--sort".into(), String::new()]),
            ["bpm", "title"],
            "{cmd}"
        );
        assert_eq!(
            app().complete(vec![cmd.to_string(), "-s".into(), "t".into()]),
            ["title"],
            "{cmd}"
        );
        assert_eq!(
            error_of(vec![cmd.to_string(), "--sort".into(), "bmp".into()]),
            "couldn't parse `bmp`: `--sort`: expected `bpm` or `title`; did you mean `bpm`?",
            "{cmd}"
        );
        let (_, inv) = app()
            .parse(vec![cmd.to_string(), "-s".into(), "bpm".into()])
            .expect("parses");
        assert_eq!(inv.path(), [cmd]);
    }
    assert_eq!(app().complete(cli! { group al }), ["album"]);
}

#[test]
fn parser_with_help_drives_alone() {
    let p = <SortOpt as ArgHelp>::parser_with_help("x").to_options();
    let s = p.run_inner(&["--sort", "title"][..]).expect("parses");
    assert_eq!(s.0, Some(SortField::Title));
    let help = <SortOpt as ArgHelp>::parser_with_help("x")
        .to_options()
        .run_inner(&["--help"][..])
        .expect_err("help")
        .unwrap_stdout();
    assert!(
        help.contains("-s, --sort=FIELD  x [bpm, title]\n"),
        "{help}"
    );
    // `Arg::parser` is the same chain, with the type's own help.
    let help = <SortOpt as Arg>::parser()
        .to_options()
        .run_inner(&["--help"][..])
        .expect_err("help")
        .unwrap_stdout();
    assert!(
        help.contains("-s, --sort=FIELD  Sort the listing by this field [bpm, title]\n"),
        "{help}"
    );
}
