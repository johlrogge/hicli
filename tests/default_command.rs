//! A group's default command (ADR-011, johlrogge/hicli#4), through the macro
//! surface: `#[command(default)]` inside a `#[group]`.
//!
//! The acceptance spec is mdma-cli's `mdma search`: a command with a free-text
//! `QUERY` and a long list of filters that *also* holds a subcommand,
//! `mdma search fact-values-for <FACT-TYPE>` — which clap expresses as
//! `Search { …, #[command(subcommand)] subcommand: Option<SearchSubcommands> }`.
//! The fixture mirrors mdma's `Commands::Search` filter for filter, and the
//! tests port mdma's `search_added_hyphen_value_parses`,
//! `search_started_hyphen_value_parses` and `search_stopped_tilde_parses`, then
//! pin what hicli adds: which command runs, `Invocation::path`, help and
//! completion.
//!
//! mdma's three date filters use clap's `allow_hyphen_values`, which accepts
//! `--added -7`. hicli keeps the standard rule that a word starting with `-` is
//! an option (#29): it takes the glued `--added=-7` with a plain `argument`,
//! and rejects `--added -7` with bpaf's hint to write `--added=-7` (see
//! `src/arg.rs` and `tests/hyphen_values.rs`).
//!
//! Everything is in-process: `App::parse`, `App::run_with_args` and
//! `App::complete`, driven by `cli!` or an arg slice.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{construct, long, pure, Bpaf, ParseFailure, Parser};
use hicli::{app, cli, command, exit_code_for, group, App, Arg, Failure, Outcome};

// --- the consumer's argument types --------------------------------------------

/// mdma's text filters and switches (`Commands::Search`, minus the three dates).
#[derive(Debug, Clone, Default, PartialEq, Bpaf, Arg)]
pub struct Filters {
    /// Filter by artist name
    #[bpaf(long, argument("ARTIST"))]
    artist: Option<String>,
    /// Filter by track title
    #[bpaf(long, argument("TITLE"))]
    title: Option<String>,
    /// Filter by album name
    #[bpaf(long, argument("ALBUM"))]
    album: Option<String>,
    /// Filter by label name
    #[bpaf(long, argument("LABEL"))]
    label: Option<String>,
    /// Filter by main genre (e.g. "Electronic", "Techno")
    #[bpaf(long, argument("GENRE"))]
    genre: Option<String>,
    /// Filter by style descriptor — matches if any descriptor matches
    #[bpaf(long, argument("STYLE"))]
    style: Option<String>,
    /// Filter by BPM. Formats: 128  128+-4  124..132  128+2  128-2
    #[bpaf(long, argument("BPM"))]
    bpm: Option<String>,
    /// Filter by musical key. Formats: Am  "A minor"  8B  8B+-1  8B+-1~
    #[bpaf(long, argument("KEY"))]
    key: Option<String>,
    /// Filter by duration. Formats: 7m  7m15s  >5m  <8m  6m..8m
    #[bpaf(long, argument("DURATION"))]
    duration: Option<String>,
    /// Filter by release year. Formats: 2022  2019..2022
    #[bpaf(long, argument("YEAR"))]
    year: Option<String>,
    /// Filter by source (bandcamp, beatport, upload)
    #[bpaf(long, argument("SOURCE"))]
    source: Option<String>,
    /// Ignore stdin (don't read piped hashes as intersection filter)
    #[bpaf(long("no-stdin"), switch)]
    no_stdin: bool,
    /// Filter by playback history. `never` matches tracks with no started fact
    #[bpaf(long, argument("PLAYED"))]
    played: Option<String>,
    /// Filter by DJ role
    #[bpaf(long, argument("ROLE"))]
    role: Option<String>,
    /// Filter by energy level (1-10). Formats: 7  5..8  7+-2
    #[bpaf(long, argument("ENERGY"))]
    energy: Option<String>,
    /// Invert the search results — return tracks that do NOT match the filters
    #[bpaf(long, switch)]
    not: bool,
}

/// mdma's date filters, each of which may start with `-`, written glued
/// (`--added=-7`).
///
/// `Arg` is written by hand — the escape hatch for what `#[derive(Bpaf)]`
/// cannot spell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dates {
    started: Option<String>,
    stopped: Option<String>,
    added: Option<String>,
}

impl Arg for Dates {
    fn parser() -> impl Parser<Self> {
        let started = long("started")
            .help("Filter by last started date. Format: N/A, >2026-02, <2026, 2026-01..2026-06")
            .argument::<String>("DATE")
            .optional();
        let stopped = long("stopped")
            .help("Filter by last stopped date. Same format as --started")
            .argument::<String>("DATE")
            .optional();
        let added = long("added")
            .help("Filter by date added to library: -7 (7 days ago), ~ (today), -7..~")
            .argument::<String>("DATE")
            .optional();
        construct!(Dates {
            started,
            stopped,
            added
        })
    }
}

/// mdma: `query: Option<String>` — the positional, which must come last.
#[derive(Debug, Clone, Default, PartialEq, Bpaf, Arg)]
pub struct Query(
    /// Free-text query applied to all text fields (title, artist, album, label, genre)
    #[bpaf(positional("QUERY"))]
    Option<String>,
);

/// mdma: `FactValuesFor { fact_type: String }`.
#[derive(Debug, Clone, PartialEq, Bpaf, Arg)]
pub struct FactType(
    /// Fact type to inspect (e.g. MainGenre, Label, Key, Source, BPM, StyleDescriptor)
    #[bpaf(positional("FACT-TYPE"))]
    String,
);

// --- the context ------------------------------------------------------------------

/// What ran, with the values it received.
#[derive(Debug, Clone, PartialEq)]
enum Ran {
    Search(Box<Filters>, Dates, Query),
    FactValuesFor(FactType),
}

/// The shared context. Every command records what it received.
pub struct Ctx {
    ran: Rc<RefCell<Vec<Ran>>>,
}

// --- the tree -----------------------------------------------------------------

/// Search for tracks
#[group]
pub mod search {
    use super::*;

    /// List all distinct values stored for a fact type.
    ///
    /// # Examples
    ///
    /// ```sh
    /// mdma search fact-values-for genre
    /// mdma search fact-values-for label | grep -i "ost"
    /// ```
    #[command]
    pub fn fact_values_for(t: FactType, ctx: &Ctx) -> Outcome {
        ctx.ran.borrow_mut().push(Ran::FactValuesFor(t));
        Ok(())
    }

    /// Search by free text and filters
    #[command(default)]
    pub fn by_text(f: Filters, d: Dates, q: Query, ctx: &Ctx) -> Outcome {
        ctx.ran.borrow_mut().push(Ran::Search(Box::new(f), d, q));
        Ok(())
    }
}

/// mdma: `Yes { #[arg(short, long)] yes: bool }`.
#[derive(Debug, Clone, PartialEq, Bpaf, Arg)]
pub struct Yes(
    /// Do not ask for confirmation
    #[bpaf(short('y'), long("yes"), switch)]
    bool,
);

/// Files that duplicate merges moved aside
#[group]
pub mod archive {
    use super::*;

    /// Permanently delete everything in the archive
    #[command]
    pub fn empty(_yes: Yes, _ctx: &Ctx) -> Outcome {
        Ok(())
    }

    /// List the archive
    #[command(default)]
    pub fn list(_ctx: &Ctx) -> Outcome {
        Ok(())
    }
}

/// Files that duplicate merges moved aside
///
/// About the archive.
#[group]
pub mod described {
    use super::*;

    /// Permanently delete everything in the archive
    #[command]
    pub fn empty(_yes: Yes, _ctx: &Ctx) -> Outcome {
        Ok(())
    }

    /// List the archive
    #[command(default)]
    pub fn list(_ctx: &Ctx) -> Outcome {
        Ok(())
    }
}

/// Only a default
#[group]
pub mod lone {
    use super::*;

    /// List the archive
    #[command(default)]
    pub fn list(_ctx: &Ctx) -> Outcome {
        Ok(())
    }
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [search, archive, described, lone] }
}

// --- seams ----------------------------------------------------------------------

/// Run `args` through the whole run path; the exit code and what ran.
fn run(args: &[&str]) -> (u8, Vec<Ran>) {
    let ran = Rc::new(RefCell::new(Vec::new()));
    let for_ctx = Rc::clone(&ran);
    let code = app().run_with_args(args.iter().copied(), move |()| -> Result<Ctx, Failure> {
        Ok(Ctx { ran: for_ctx })
    });
    let ran = ran.borrow().clone();
    (code, ran)
}

/// Run `args`, asserting success; what ran.
fn ok(args: &[&str]) -> Ran {
    let (code, mut ran) = run(args);
    assert_eq!(code, 0, "{args:?}");
    assert_eq!(ran.len(), 1, "{args:?}: {ran:?}");
    ran.remove(0)
}

/// What the default command receives for `filters`, `dates` and `query`.
fn searched(filters: Filters, dates: Dates, query: Option<&str>) -> Ran {
    Ran::Search(Box::new(filters), dates, Query(query.map(str::to_string)))
}

/// The selected command's path.
fn path(args: &[&str]) -> Vec<&'static str> {
    match app().parse(args.iter().copied()) {
        Ok((_, inv)) => inv.path().to_vec(),
        Err(e) => panic!("{args:?} did not parse: {e:?}"),
    }
}

/// The usage error for `args`, asserting it is one (stderr, exit code 2).
fn usage_error(args: &[&str]) -> String {
    match app().parse(args.iter().copied()) {
        Ok((_, inv)) => panic!("{args:?} parsed, as {:?}", inv.path()),
        Err(e @ ParseFailure::Stderr(..)) => {
            assert_eq!(exit_code_for(&e), 2, "{args:?}");
            e.unwrap_stderr()
        }
        Err(e) => panic!("{args:?}: not a usage error: {e:?}"),
    }
}

/// The help page for `args` (which end in `--help`).
fn help(args: &[&str]) -> String {
    match app().parse(args.iter().copied()) {
        Ok((_, inv)) => panic!("{args:?} parsed, as {:?}", inv.path()),
        Err(e @ ParseFailure::Stdout(..)) => e.unwrap_stdout(),
        Err(e) => panic!("{args:?}: not help: {e:?}"),
    }
}

// --- the default runs when no sibling is named ----------------------------------

#[test]
fn a_bare_group_runs_the_default() {
    assert_eq!(
        ok(&["search"]),
        searched(Filters::default(), Dates::default(), None)
    );
    assert_eq!(path(&["search"]), ["search"]);
}

#[test]
fn a_query_and_a_filter_run_the_default() {
    let want = searched(
        Filters {
            artist: Some("Bowie".into()),
            ..Filters::default()
        },
        Dates::default(),
        Some("jazz"),
    );
    assert_eq!(ok(&["search", "jazz", "--artist", "Bowie"]), want);
    assert_eq!(ok(&["search", "--artist", "Bowie", "jazz"]), want);
    // The default answers to no word: its path is the group's.
    assert_eq!(path(&["search", "jazz", "--artist", "Bowie"]), ["search"]);
    let (_, inv) = app().parse(cli! { search jazz }).expect("parses");
    assert_eq!(inv.name(), "search");
}

#[test]
fn switches_and_several_filters_reach_the_default() {
    assert_eq!(
        ok(&[
            "search",
            "--genre",
            "Techno",
            "--bpm",
            "124..132",
            "--no-stdin",
            "--not",
        ]),
        searched(
            Filters {
                genre: Some("Techno".into()),
                bpm: Some("124..132".into()),
                no_stdin: true,
                not: true,
                ..Filters::default()
            },
            Dates::default(),
            None,
        )
    );
}

#[test]
fn a_named_sibling_wins_its_own_word() {
    assert_eq!(
        ok(&["search", "fact-values-for", "genre"]),
        Ran::FactValuesFor(FactType("genre".into()))
    );
    assert_eq!(
        path(&["search", "fact-values-for", "genre"]),
        ["search", "fact-values-for"]
    );
}

#[test]
fn a_named_sibling_missing_its_argument_is_its_own_error() {
    // Not the default with QUERY = "fact-values-for": the named command's own
    // missing argument.
    assert_eq!(
        usage_error(&["search", "fact-values-for"]),
        "expected `FACT-TYPE`, pass `--help` for usage information"
    );
    assert_eq!(run(&["search", "fact-values-for"]).0, 2);
}

#[test]
fn double_dash_searches_for_a_sibling_name() {
    assert_eq!(
        ok(&["search", "--", "fact-values-for"]),
        searched(
            Filters::default(),
            Dates::default(),
            Some("fact-values-for")
        )
    );
    assert_eq!(path(&["search", "--", "fact-values-for"]), ["search"]);
}

// --- mdma's date-arg hyphen-value tests -----------------------------------------

#[test]
fn search_added_glued_hyphen_value_parses() {
    // `mdma search --added=-7`: the glued form reads a dash-led value.
    assert_eq!(
        ok(&["search", "--added=-7"]),
        searched(
            Filters::default(),
            Dates {
                added: Some("-7".into()),
                ..Dates::default()
            },
            None,
        )
    );
}

#[test]
fn search_added_two_word_hyphen_value_is_a_usage_error() {
    // #29: the two-word form is a usage error, and bpaf names the glued form.
    assert_eq!(
        usage_error(&["search", "--added", "-7"]),
        "`--added` requires an argument `DATE`, got a flag `-7`, \
         try `--added=-7` to use it as an argument"
    );
    assert_eq!(run(&["search", "--added", "-7"]).0, 2);
}

#[test]
fn search_started_glued_hyphen_value_parses() {
    assert_eq!(
        ok(&["search", "--started=-7"]),
        searched(
            Filters::default(),
            Dates {
                started: Some("-7".into()),
                ..Dates::default()
            },
            None,
        )
    );
}

#[test]
fn search_stopped_tilde_parses() {
    assert_eq!(
        ok(&["search", "--stopped", "~"]),
        searched(
            Filters::default(),
            Dates {
                stopped: Some("~".into()),
                ..Dates::default()
            },
            None,
        )
    );
}

#[test]
fn hyphen_values_mix_with_a_query_and_the_equals_spelling() {
    assert_eq!(
        ok(&["search", "--added=-7..~", "--genre", "jazz", "miles"]),
        searched(
            Filters {
                genre: Some("jazz".into()),
                ..Filters::default()
            },
            Dates {
                added: Some("-7..~".into()),
                ..Dates::default()
            },
            Some("miles"),
        )
    );
}

/// Known limit (documented in `src/lib.rs`): a typo of a named sibling in
/// front of the default is silently the default's `QUERY` — there is nothing
/// to tell it apart from a search for that word. Pinned so a change shows.
#[test]
fn known_limit_a_typo_of_a_sibling_is_a_query() {
    assert_eq!(
        ok(&["search", "fact-valus-for"]),
        searched(Filters::default(), Dates::default(), Some("fact-valus-for"))
    );
}

#[test]
fn a_second_positional_is_a_usage_error() {
    assert_eq!(
        usage_error(&["search", "jazz", "extra"]),
        "`extra` is not expected in this context"
    );
}

// --- help -----------------------------------------------------------------------

#[test]
fn search_help_shows_both_forms_the_options_and_the_commands() {
    let h = help(&["search", "--help"]);
    // bpaf wraps a long usage at 100 columns, mid-token (`[` / `--genre`), so
    // the usage is the paragraph after `Usage: `, unwrapped.
    let usage: String = h
        .split("\n\n")
        .find(|p| p.starts_with("Usage: "))
        .unwrap_or_else(|| panic!("no usage in {h}"))
        .lines()
        .collect();
    // Both forms: a named command, or the default's options and QUERY.
    assert!(usage.starts_with("Usage: search (COMMAND ... | "), "{h}");
    assert!(usage.contains("[--artist=ARTIST]"), "{h}");
    assert!(usage.ends_with("[QUERY])"), "{h}");
    // The default's arguments, under its one-line help.
    assert!(h.contains("Search by free text and filters"), "{h}");
    for opt in [
        "--artist=ARTIST",
        "--genre=GENRE",
        "--no-stdin",
        "--not",
        "--added=DATE",
        "--started=DATE",
        "--stopped=DATE",
        "QUERY",
    ] {
        assert!(h.contains(opt), "missing {opt} in {h}");
    }
    // Each date filter is listed once, as a plain `--added=DATE` argument.
    let added = h
        .lines()
        .filter(|l| l.trim_start().starts_with("--added"))
        .count();
    assert_eq!(added, 1, "{h}");
    // The named sibling, under the commands.
    let commands = h
        .split("Available commands:")
        .nth(1)
        .unwrap_or_else(|| panic!("no commands section in {h}"));
    assert!(commands.contains("fact-values-for"), "{h}");
    assert!(commands.contains("List all distinct values"), "{h}");
}

#[test]
fn search_help_describes_the_default_once() {
    let h = help(&["search", "--help"]);
    assert_eq!(
        h.matches("Search by free text and filters").count(),
        1,
        "{h}"
    );
}

/// #19: a default without options is described under the usage, and naming a
/// command is optional.
#[test]
fn a_default_without_options_is_described_under_the_usage() {
    assert_eq!(
        help(&["archive", "--help"]),
        "Files that duplicate merges moved aside\n\n\
         Usage: archive [COMMAND ...]\n\n\
         List the archive\n\n\
         Available options:\n    -h, --help  Prints help information\n\n\
         Available commands:\n    \
         empty       Permanently delete everything in the archive\n"
    );
    assert_eq!(path(&["archive"]), ["archive"]);
}

#[test]
fn a_default_without_options_comes_before_the_groups_details() {
    for args in [
        &["described", "--help"][..],
        &["described", "--help", "--help"][..],
    ] {
        let h = help(args);
        assert!(
            h.contains(
                "Usage: described [COMMAND ...]\n\nList the archive\n\nAbout the archive.\n"
            ),
            "{h}"
        );
    }
}

#[test]
fn a_group_of_only_a_bare_default_has_an_unbracketed_usage() {
    let h = help(&["lone", "--help"]);
    assert!(h.contains("\nUsage: lone\n\nList the archive\n"), "{h}");
}

#[test]
fn a_named_siblings_help_is_its_own_page() {
    let h = help(&["search", "fact-values-for", "--help"]);
    assert!(h.contains("Usage: search fact-values-for FACT-TYPE"), "{h}");
    assert!(h.contains("mdma search fact-values-for genre"), "{h}");
    assert!(!h.contains("--artist"), "{h}");
}

// --- completion -----------------------------------------------------------------

#[test]
fn the_empty_word_offers_the_sibling_once_and_the_defaults_options() {
    let got = app().complete(cli! { search "" });
    assert_eq!(
        got.iter().filter(|c| *c == "fact-values-for").count(),
        1,
        "{got:?}"
    );
    for opt in ["--artist", "--genre", "--added", "--not"] {
        assert!(got.contains(&opt.to_string()), "missing {opt} in {got:?}");
    }
}

#[test]
fn a_typed_prefix_of_the_sibling_completes_to_it() {
    assert_eq!(app().complete(cli! { search f }), ["fact-values-for"]);
    // Not once an option is given: the named command is unreachable there.
    let got = app().complete(cli! { search "--artist" x f });
    assert!(!got.contains(&"fact-values-for".to_string()), "{got:?}");
}

/// Regression: bpaf's revision-1 payload has no header line, and
/// `App::complete` once dropped its first candidate — `--artist` here,
/// `fact-values-for` at `search ""`.
#[test]
fn every_matching_option_of_the_default_is_offered() {
    assert_eq!(
        app().complete(cli! { search "--a" }),
        ["--artist", "--album", "--added"]
    );
    let got = app().complete(cli! { search "" });
    assert!(got.contains(&"fact-values-for".to_string()), "{got:?}");
}
