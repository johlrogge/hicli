//! The bpaf contract behind ADR-011 (curried commands, johlrogge/hicli#1;
//! default commands, johlrogge/hicli#4): what raw bpaf (pinned against
//! 0.9.27) does with the two node shapes hicli builds them from. No hicli API
//! is exercised — every parser here is plain bpaf, reached through the
//! `hicli::bpaf` re-export — so the pinned outputs are facts about bpaf that
//! hicli's design lives with.
//!
//! This began as the ADR's spike and is kept as a **permanent contract test**:
//! `hicli::group_args`, default commands and `App::complete` are built on these
//! answers. **Re-run it on every bpaf upgrade** — a changed answer here is a
//! changed assumption in `src/command.rs` or `src/app.rs`, and has to be read
//! as one before the upgrade lands.
//!
//! The two shapes:
//!
//! - **`source`** — a command node holding a positional `SOURCE-NAME` followed
//!   by *bound* subcommands (`sync`, `status`, `cancel <ID>`), next to a *fixed*
//!   sibling subcommand (`list`) that takes no source name.
//! - **`search`** — a command node holding a named subcommand
//!   (`fact-values-for <TYPE>`) and a *default* branch that is not a command at
//!   all: `[--artist ARTIST] [--added DAYS] [QUERY]`.
//!
//! Every test renders the raw `run_inner` result (help/usage text exactly as
//! bpaf prints it; completion through revision 1, the in-process-safe protocol
//! `App::complete` uses) and pins it, so a bpaf upgrade that changes any of
//! these answers fails loudly here.

use hicli::bpaf::{
    any, construct,
    doc::{Doc, MetaInfo},
    long, positional, pure, pure_with, OptionParser, ParseFailure, Parser,
};
use std::cell::RefCell;
use std::fmt::Debug;

// --- the model ---------------------------------------------------------------
//
// The tests assert the `Debug` rendering; `PartialEq` is derived only so the
// fields count as read (derived `Debug`/`Clone` do not, for `dead_code`).

/// A subcommand bound to a source name.
#[derive(Debug, Clone, PartialEq)]
enum SrcCmd {
    Sync,
    Status,
    Cancel(String),
}

/// What `source …` selected.
#[derive(Debug, Clone, PartialEq)]
enum Source {
    /// The fixed sibling: `source list`.
    List,
    /// The curried branch: `source SOURCE-NAME <cmd>`.
    Bound(String, SrcCmd),
}

/// What `search …` selected.
#[derive(Debug, Clone, PartialEq)]
enum Search {
    FactValues(String),
    Default {
        query: Option<String>,
        artist: Option<String>,
        added: Option<String>,
    },
}

/// The fixed siblings' names: a source may not be called this.
const RESERVED: &[&str] = &["list"];

/// The bound subcommands: `sync`, `status`, `cancel <ID>`.
fn bound_cmds() -> impl Parser<SrcCmd> {
    let sync = pure(SrcCmd::Sync)
        .to_options()
        .descr("sync a source")
        .command("sync");
    let status = pure(SrcCmd::Status)
        .to_options()
        .descr("show a source's status")
        .command("status");
    let cancel = positional::<String>("ID")
        .help("job id")
        .map(SrcCmd::Cancel)
        .to_options()
        .descr("cancel a job")
        .command("cancel");
    construct!([sync, status, cancel])
}

/// `SOURCE-NAME`, completed from a fixed list.
fn source_name() -> impl Parser<String> {
    positional::<String>("SOURCE-NAME")
        .help("the source")
        .complete(|s: &String| {
            ["bandcamp", "bandit", "discogs"]
                .into_iter()
                .filter(|c| c.starts_with(s.as_str()))
                .map(|c| (c, None))
                .collect::<Vec<_>>()
        })
}

/// The curried branch: `SOURCE-NAME` then a bound subcommand.
fn bound() -> impl Parser<Source> {
    let pos = source_name();
    let cmds = bound_cmds();
    construct!(pos, cmds).map(|(n, c)| Source::Bound(n, c))
}

/// The fixed sibling `list`.
fn list() -> impl Parser<Source> {
    pure(Source::List)
        .to_options()
        .descr("list sources")
        .command("list")
}

/// S4's hidden guard: fails when the *front* item is a reserved word, consumes
/// nothing otherwise.
fn reserved_guard() -> impl Parser<()> {
    any::<String, _, _>("", |s: String| RESERVED.contains(&s.as_str()).then_some(()))
        .optional()
        .guard(Option::is_none, "reserved word")
        .hide()
        .map(|_| ())
}

/// Wrap `inner` as the command `name` under an otherwise empty root.
fn top<T: 'static>(inner: impl Parser<T> + 'static, name: &'static str) -> OptionParser<T> {
    let c = inner.to_options().descr("the node").command(name);
    construct!([c]).to_options()
}

/// `source SOURCE-NAME <cmd>` with no fixed sibling.
fn source_bound_only() -> OptionParser<Source> {
    top(bound(), "source")
}

/// `source (list | SOURCE-NAME <cmd>)`, fixed sibling first or last.
fn source_with_sibling(fixed_first: bool) -> OptionParser<Source> {
    let fixed = list();
    let b = bound();
    if fixed_first {
        top(construct!([fixed, b]), "source")
    } else {
        top(construct!([b, fixed]), "source")
    }
}

/// As [`source_with_sibling`], with the reserved-word guard in front of the
/// positional branch.
fn source_guarded(fixed_first: bool) -> OptionParser<Source> {
    let g = reserved_guard();
    let b = bound();
    let guarded = construct!(g, b).map(|((), s)| s);
    let fixed = list();
    if fixed_first {
        top(construct!([fixed, guarded]), "source")
    } else {
        top(construct!([guarded, fixed]), "source")
    }
}

/// The named subcommand `fact-values-for <TYPE>`.
fn fact_values_for() -> impl Parser<Search> {
    positional::<String>("TYPE")
        .map(Search::FactValues)
        .to_options()
        .descr("list values for a fact type")
        .command("fact-values-for")
}

/// The default branch. The positional is **last**: bpaf panics when help
/// renders ("all positional and command items must be placed in the right
/// most position") if `QUERY` precedes `--artist` in the tuple.
fn default_branch() -> impl Parser<Search> {
    let artist = long("artist")
        .help("by artist")
        .argument::<String>("ARTIST")
        .complete(|s: &String| {
            ["abba", "acdc"]
                .into_iter()
                .filter(|c| c.starts_with(s.as_str()))
                .map(|c| (c, None))
                .collect::<Vec<_>>()
        })
        .optional();
    let added = long("added").argument::<String>("DAYS").optional();
    let query = positional::<String>("QUERY").help("free text").optional();
    construct!(artist, added, query)
        .map(|(artist, added, query)| Search::Default {
            query,
            artist,
            added,
        })
        .group_help("Search options:")
}

/// `search (fact-values-for TYPE | default)`, named first or last.
fn search(named_first: bool) -> OptionParser<Search> {
    let f = fact_values_for();
    let d = default_branch();
    if named_first {
        top(construct!([f, d]), "search")
    } else {
        top(construct!([d, f]), "search")
    }
}

// --- rendering ---------------------------------------------------------------

/// The raw outcome of `args`, tagged by stream.
fn show<T: Debug>(p: OptionParser<T>, args: &[&str]) -> String {
    match p.run_inner(args) {
        Ok(v) => format!("Ok({v:?})"),
        Err(e @ ParseFailure::Stderr(..)) => format!("STDERR: {}", e.unwrap_stderr()),
        Err(e @ ParseFailure::Stdout(..)) => format!("STDOUT:\n{}", e.unwrap_stdout()),
        Err(ParseFailure::Completion(s)) => format!("COMPLETION:\n{s}"),
    }
}

/// The raw revision-1 completion payload for `args` (last word = the one being
/// completed).
fn comp<T: Debug>(p: OptionParser<T>, args: &[&str]) -> String {
    let mut v = vec!["--bpaf-complete-rev=1"];
    v.extend_from_slice(args);
    show(p, &v)
}

// --- S1: a positional then subcommands ---------------------------------------

#[test]
fn s1_positional_then_subcommand_parses() {
    assert_eq!(
        show(source_bound_only(), &["source", "bandcamp", "sync"]),
        r#"Ok(Bound("bandcamp", Sync))"#
    );
    assert_eq!(
        show(source_bound_only(), &["source", "bandcamp", "cancel", "7"]),
        r#"Ok(Bound("bandcamp", Cancel("7")))"#
    );
}

#[test]
fn s1_a_missing_subcommand_and_a_swapped_order_are_usage_errors() {
    assert_eq!(
        show(source_bound_only(), &["source", "bandcamp"]),
        "STDERR: expected `COMMAND ...`, pass `--help` for usage information"
    );
    // `sync` is taken as SOURCE-NAME; then `bandcamp` is not a command.
    assert_eq!(
        show(source_bound_only(), &["source", "sync", "bandcamp"]),
        "STDERR: expected `COMMAND ...`, got `bandcamp`. Pass `--help` for usage information"
    );
}

// --- S2: a fixed sibling next to the positional branch -----------------------

#[test]
fn s2_the_fixed_sibling_wins_alone_but_the_positional_branch_wins_a_longer_match() {
    for fixed_first in [true, false] {
        let p = || source_with_sibling(fixed_first);
        assert_eq!(show(p(), &["source", "list"]), "Ok(List)");
        assert_eq!(
            show(p(), &["source", "bandcamp", "sync"]),
            r#"Ok(Bound("bandcamp", Sync))"#
        );
        // Longest match wins, in either order: `list` is taken as a source name.
        assert_eq!(
            show(p(), &["source", "list", "sync"]),
            r#"Ok(Bound("list", Sync))"#
        );
        assert_eq!(
            show(p(), &["source", "lst"]),
            "STDERR: no such command or positional: `lst`, did you mean `list`?"
        );
    }
}

// --- S3: a named subcommand next to a default branch -------------------------

#[test]
fn s3_default_branch_outcomes_are_order_independent() {
    for named_first in [true, false] {
        let p = || search(named_first);
        // (a)
        assert_eq!(
            show(p(), &["search"]),
            "Ok(Default { query: None, artist: None, added: None })"
        );
        // (b)
        assert_eq!(
            show(p(), &["search", "jazz"]),
            r#"Ok(Default { query: Some("jazz"), artist: None, added: None })"#
        );
        // (c)
        assert_eq!(
            show(p(), &["search", "fact-values-for", "genre"]),
            r#"Ok(FactValues("genre"))"#
        );
        // (d) CRITICAL: the named command's own error, not QUERY=fact-values-for.
        assert_eq!(
            show(p(), &["search", "fact-values-for"]),
            "STDERR: expected `TYPE`, pass `--help` for usage information"
        );
        // (e) a default-branch option before the name: the default branch takes
        // `fact-values-for` as QUERY and `genre` is left over.
        assert_eq!(
            show(
                p(),
                &["search", "--artist", "X", "fact-values-for", "genre"]
            ),
            "STDERR: `genre` is not expected in this context"
        );
        // (f) `--` is the escape hatch to search for a reserved word.
        assert_eq!(
            show(p(), &["search", "--", "fact-values-for"]),
            r#"Ok(Default { query: Some("fact-values-for"), artist: None, added: None })"#
        );
        // (g) a dash-led value is written glued inside the default branch; the
        // two-word form is a usage error naming the glued form (#29).
        assert_eq!(
            show(p(), &["search", "--added=-7", "jazz"]),
            r#"Ok(Default { query: Some("jazz"), artist: None, added: Some("-7") })"#
        );
        assert_eq!(
            show(p(), &["search", "--added", "-7"]),
            "STDERR: `--added` requires an argument `DAYS`, got a flag `-7`, \
             try `--added=-7` to use it as an argument"
        );
        // (h) a typo of the named command is silently a query — no suggestion.
        assert_eq!(
            show(p(), &["search", "fact-valus-for"]),
            r#"Ok(Default { query: Some("fact-valus-for"), artist: None, added: None })"#
        );
        assert_eq!(
            show(p(), &["search", "jazz", "extra"]),
            "STDERR: `extra` is not expected in this context"
        );
    }
}

// --- S4: the reserved-word guard ---------------------------------------------

#[test]
fn s4_the_guard_restores_sibling_wins_in_either_order() {
    for fixed_first in [true, false] {
        let p = || source_guarded(fixed_first);
        assert_eq!(show(p(), &["source", "list"]), "Ok(List)");
        assert_eq!(
            show(p(), &["source", "list", "sync"]),
            "STDERR: `sync` is not expected in this context"
        );
        assert_eq!(
            show(p(), &["source", "bandcamp", "sync"]),
            r#"Ok(Bound("bandcamp", Sync))"#
        );
        // `any` inspects only the front item: a reserved word later is not
        // guarded (and is simply not a bound command here).
        assert_eq!(
            show(p(), &["source", "bandcamp", "list"]),
            "STDERR: expected `COMMAND ...`, got `bandcamp`. Pass `--help` for usage information"
        );
        assert_eq!(
            show(p(), &["source", "lst"]),
            "STDERR: no such command or positional: `lst`, did you mean `list`?"
        );
    }
}

#[test]
fn s4_the_guard_does_not_leak_into_help_or_completion() {
    // Byte-identical to the unguarded node, in both stdout channels.
    for fixed_first in [true, false] {
        assert_eq!(
            show(source_guarded(fixed_first), &["source", "--help"]),
            show(source_with_sibling(fixed_first), &["source", "--help"])
        );
        assert_eq!(
            comp(source_guarded(fixed_first), &["source", ""]),
            comp(source_with_sibling(fixed_first), &["source", ""])
        );
    }
}

// --- S5: help ----------------------------------------------------------------

const SOURCE_HELP: &str = "\
the node

Usage: source (COMMAND ... | SOURCE-NAME COMMAND ...)

Available positional items:
    SOURCE-NAME  the source

Available options:
    -h, --help   Prints help information

Available commands:
    list         list sources
    sync         sync a source
    status       show a source's status
    cancel       cancel a job
";

#[test]
fn s5_node_help_flattens_fixed_and_bound_commands_into_one_list() {
    assert_eq!(
        show(source_with_sibling(true), &["source", "--help"]),
        format!("STDOUT:\n{SOURCE_HELP}")
    );
    // After a source name, `--help` is still the node's page.
    assert_eq!(
        show(source_with_sibling(true), &["source", "bandcamp", "--help"]),
        format!("STDOUT:\n{SOURCE_HELP}")
    );
}

#[test]
fn s5_a_bound_commands_usage_omits_the_curried_positional() {
    // Raw bpaf, pinned as is: its derived line drops the curried value and,
    // for a command without arguments, ends in a space (`sync `). Both are
    // why hicli now renders every usage line itself (ADR-013, #28); re-run
    // this on a bpaf upgrade.
    assert_eq!(
        show(
            source_with_sibling(true),
            &["source", "bandcamp", "sync", "--help"]
        ),
        "STDOUT:\nsync a source\n\nUsage: source sync \n\n\
         Available options:\n    -h, --help  Prints help information\n"
    );
}

#[test]
fn s5_search_help_shows_both_forms_and_the_default_group() {
    let expected = "\
the node

Usage: search (COMMAND ... | [--artist=ARTIST] [--added=DAYS] [QUERY])

Search options:
        --artist=ARTIST  by artist
        --added=DAYS
    QUERY                free text

Available options:
    -h, --help           Prints help information

Available commands:
    fact-values-for      list values for a fact type
";
    assert_eq!(
        show(search(true), &["search", "--help"]),
        format!("STDOUT:\n{expected}")
    );
    // A query before `--help` changes nothing.
    assert_eq!(
        show(search(true), &["search", "jazz", "--help"]),
        format!("STDOUT:\n{expected}")
    );
}

// --- S6: completion ----------------------------------------------------------

#[test]
fn s6_source_completion_mixes_fixed_values_and_bound_commands() {
    // At the SOURCE-NAME slot: the fixed command, the completer's values, and
    // the bound commands — which are not valid here.
    assert_eq!(
        comp(source_with_sibling(true), &["source", ""]),
        "COMPLETION:\nlist\tlist sources\n\tthe source\nbandcamp\nbandit\ndiscogs\n\
         sync\tsync a source\nstatus\tshow a source's status\ncancel\tcancel a job\n"
    );
    assert_eq!(
        comp(source_with_sibling(true), &["source", "b"]),
        "COMPLETION:\n\tthe source\nbandcamp\nbandit\n"
    );
    assert_eq!(
        comp(source_with_sibling(true), &["source", "s"]),
        "COMPLETION:\n\tthe source\nsync\tsync a source\nstatus\tshow a source's status\n"
    );
    assert_eq!(
        comp(source_with_sibling(true), &["source", "bandcamp", ""]),
        "COMPLETION:\nsync\tsync a source\nstatus\tshow a source's status\ncancel\tcancel a job\n"
    );
}

#[test]
fn s6_a_typed_prefix_of_a_sibling_command_is_shadowed_by_the_positional() {
    // Baseline: two plain commands complete `l` to both.
    let fixed = list();
    let lost = pure(Source::List)
        .to_options()
        .descr("other")
        .command("lost");
    let baseline = top(construct!([fixed, lost]), "source");
    assert_eq!(
        comp(baseline, &["source", "l"]),
        "COMPLETION:\nlist\tlist sources\nlost\tother\n"
    );
    // With a positional alternative, `l` completes to nothing — in any order,
    // guarded or not; same for the default branch's QUERY. The payload is a
    // lone empty line: no candidates at all.
    let nothing = "COMPLETION:\n\n";
    for first in [true, false] {
        assert_eq!(comp(source_with_sibling(first), &["source", "l"]), nothing);
        assert_eq!(comp(source_guarded(first), &["source", "l"]), nothing);
        assert_eq!(comp(search(first), &["search", "f"]), nothing);
        assert_eq!(comp(search(first), &["search", "fa"]), nothing);
    }
}

#[test]
fn s6_search_completion() {
    assert_eq!(
        comp(search(true), &["search", ""]),
        "COMPLETION:\nfact-values-for\tlist values for a fact type\n\
         --artist\tby artist\n--added\n\tfree text\n"
    );
    assert_eq!(
        comp(search(true), &["search", "--a"]),
        "COMPLETION:\n--artist\tby artist\n--added\n"
    );
    assert_eq!(
        comp(search(true), &["search", "--artist", ""]),
        "COMPLETION:\n\tby artist\nabba\nacdc\n"
    );
}

// --- S7: pure_with inside a command ------------------------------------------

#[test]
fn s7_pure_with_leaves_usage_and_help_clean() {
    let p = || {
        let ctx = pure_with(|| Ok::<_, String>("x".to_string()));
        let name = positional::<String>("NAME");
        top(construct!(ctx, name).map(|(a, b)| a + &b), "seven")
    };
    assert_eq!(show(p(), &["seven", "n"]), r#"Ok("xn")"#);
    assert_eq!(
        show(p(), &["seven", "--help"]),
        "STDOUT:\nthe node\n\nUsage: seven NAME\n\n\
         Available options:\n    -h, --help  Prints help information\n"
    );
}

// --- S8: curried inside curried ----------------------------------------------

fn nested() -> OptionParser<(String, String)> {
    let b = positional::<String>("B");
    let f = pure(()).to_options().descr("f leaf").command("f");
    let inner = construct!(b, f).map(|(b, ())| b);
    let y = inner.to_options().descr("y node").command("y");
    let a = positional::<String>("A");
    top(construct!(a, y), "x")
}

#[test]
fn s8_curried_inside_curried_parses() {
    assert_eq!(
        show(nested(), &["x", "A", "y", "B", "f"]),
        r#"Ok(("A", "B"))"#
    );
    // … and, as in S5, the inner usage drops the outer positional `A`.
    assert_eq!(
        show(nested(), &["x", "A", "y", "--help"]),
        "STDOUT:\ny node\n\nUsage: x y B COMMAND ...\n\n\
         Available options:\n    -h, --help  Prints help information\n\n\
         Available commands:\n    f           f leaf\n"
    );
}

// --- S9: bpaf's own suggestion -----------------------------------------------

#[test]
fn s9_a_fixed_sibling_degrades_the_bound_command_typo_error() {
    // Alone, bpaf suggests the bound command.
    assert_eq!(
        show(source_bound_only(), &["source", "bandcamp", "snyc"]),
        "STDERR: no such command or positional: `snyc`, did you mean `sync`?"
    );
    // Next to a fixed sibling — any order, guarded or not — the error blames
    // the source name instead, with no suggestion.
    let blame =
        "STDERR: expected `COMMAND ...`, got `bandcamp`. Pass `--help` for usage information";
    for first in [true, false] {
        assert_eq!(
            show(source_with_sibling(first), &["source", "bandcamp", "snyc"]),
            blame
        );
        assert_eq!(
            show(source_guarded(first), &["source", "bandcamp", "snyc"]),
            blame
        );
        assert_eq!(
            show(source_with_sibling(first), &["source", "bandcamp"]),
            blame
        );
    }
}

// =============================================================================
// Round 2
// =============================================================================

const SOURCES: &[&str] = &["bandcamp", "bandit", "discogs"];
/// The fixed siblings of `source`, as completion candidates.
const SOURCE_FIXED: &[(&str, Option<&str>)] = &[("list", Some("list sources"))];
/// The named siblings of `search`'s default branch, as completion candidates.
const SEARCH_FIXED: &[(&str, Option<&str>)] =
    &[("fact-values-for", Some("list values for a fact type"))];

/// The raw completion payload for `args` at protocol revision `rev`.
fn comp_rev<T: Debug>(rev: usize, p: OptionParser<T>, args: &[&str]) -> String {
    let flag = format!("--bpaf-complete-rev={rev}");
    let mut v = vec![flag.as_str()];
    v.extend_from_slice(args);
    show(p, &v)
}

fn prefixed(
    word: &str,
    cands: &'static [(&'static str, Option<&'static str>)],
) -> Vec<(&'static str, Option<&'static str>)> {
    cands
        .iter()
        .copied()
        .filter(|(c, _)| c.starts_with(word))
        .collect()
}

/// When a positional's completer also offers the fixed siblings' names.
#[derive(Clone, Copy)]
enum Fixed {
    /// Never (round 1's completer).
    No,
    /// Only once a prefix is typed.
    NonEmpty,
    /// Always.
    Always,
}

impl Fixed {
    fn offer(self, word: &str) -> bool {
        match self {
            Fixed::No => false,
            Fixed::NonEmpty => !word.is_empty(),
            Fixed::Always => true,
        }
    }
}

/// `SOURCE-NAME`, whose completer may also offer the fixed siblings.
fn source_name_with(fixed: Fixed) -> impl Parser<String> {
    positional::<String>("SOURCE-NAME")
        .help("the source")
        .complete(move |s: &String| {
            let mut v: Vec<(&str, Option<&str>)> = SOURCES
                .iter()
                .filter(|c| c.starts_with(s.as_str()))
                .map(|c| (*c, None))
                .collect();
            if fixed.offer(s) {
                v.extend(prefixed(s, SOURCE_FIXED));
            }
            v
        })
}

/// `sync [--apply]` — its `OptionParser` before `.command("sync")`.
fn sync_apply_opts() -> OptionParser<SrcCmd> {
    long("apply")
        .help("apply the plan")
        .switch()
        .map(|_| SrcCmd::Sync)
        .to_options()
        .descr("sync a source")
}

/// The bound subcommands with a caller-supplied `sync`.
fn bound_cmds_with(sync: impl Parser<SrcCmd> + 'static) -> impl Parser<SrcCmd> {
    let status = pure(SrcCmd::Status)
        .to_options()
        .descr("show a source's status")
        .command("status");
    let cancel = positional::<String>("ID")
        .help("job id")
        .map(SrcCmd::Cancel)
        .to_options()
        .descr("cancel a job")
        .command("cancel");
    construct!([sync, status, cancel])
}

/// `source (list | <guard> SOURCE-NAME <cmd>)` with the pieces supplied.
fn source_r2(
    guard: Box<dyn Parser<()>>,
    name: impl Parser<String> + 'static,
    cmds: impl Parser<SrcCmd> + 'static,
) -> OptionParser<Source> {
    let guarded = construct!(guard, name, cmds).map(|((), n, c)| Source::Bound(n, c));
    let fixed = list();
    top(construct!([fixed, guarded]), "source")
}

/// A `Doc` transformer that puts `prefix` in front of bpaf's derived usage.
fn usage_prefixed(prefix: &'static str) -> impl Fn(Doc) -> Doc {
    move |derived| {
        let mut u = Doc::default();
        u.text(prefix);
        u.doc(&derived);
        u
    }
}

/// What `with_usage` hands its closure, rendered.
fn usage_of<T>(p: OptionParser<T>) -> String {
    let seen = RefCell::new(String::new());
    let _ = p.with_usage(|d| {
        *seen.borrow_mut() = d.to_string();
        d
    });
    seen.into_inner()
}

/// The `Usage:` line of `p`'s `--help`, minus the `Usage: ` prefix.
fn usage_via_help<T: Debug>(p: OptionParser<T>) -> String {
    match p.run_inner(&["--help"][..]) {
        Err(ParseFailure::Stdout(doc, full)) => doc
            .monochrome(full)
            .lines()
            .find_map(|l| l.strip_prefix("Usage: "))
            .map(str::to_string)
            .unwrap_or_default(),
        other => format!("not help: {}", other.is_ok()),
    }
}

/// S4's guard, plus a completer on the `any`, wrapped by `hide`.
fn completing_guard(
    reserved: &'static [&'static str],
    cands: &'static [(&'static str, Option<&'static str>)],
    hide: &str,
) -> Box<dyn Parser<()>> {
    let g = any::<String, _, _>("", move |s: String| {
        reserved.contains(&s.as_str()).then_some(())
    })
    .complete(move |_: &()| cands.to_vec())
    .optional()
    .guard(Option::is_none, "reserved word")
    .map(|_| ());
    match hide {
        "hide" => g.hide().boxed(),
        "hide_usage" => g.hide_usage().boxed(),
        "custom_usage" => g.custom_usage("").boxed(),
        _ => g.boxed(),
    }
}

/// `any` alone with a prefix completer, accepting everything or only `zzz`.
fn any_alone(accept_all: bool) -> OptionParser<String> {
    let a = any::<String, _, _>("WORD", move |s: String| {
        (accept_all || s == "zzz").then_some(s)
    })
    .help("a word")
    .complete(|s: &String| prefixed(s, SOURCE_FIXED));
    top(a, "t")
}

/// `search`'s default branch with a `QUERY` completer that may offer the
/// named sibling, and an optional guard in front.
fn search_r2(guard: Option<Box<dyn Parser<()>>>, fixed: Fixed) -> OptionParser<Search> {
    let artist = long("artist")
        .help("by artist")
        .argument::<String>("ARTIST")
        .optional();
    let added = long("added").argument::<String>("DAYS").optional();
    let query = positional::<String>("QUERY")
        .help("free text")
        .complete(move |s: &String| {
            if fixed.offer(s) {
                prefixed(s, SEARCH_FIXED)
            } else {
                Vec::new()
            }
        })
        .optional();
    let d = construct!(artist, added, query).map(|(artist, added, query)| Search::Default {
        query,
        artist,
        added,
    });
    let d: Box<dyn Parser<Search>> = match guard {
        Some(g) => construct!(g, d).map(|((), s)| s).boxed(),
        None => d.boxed(),
    };
    let f = fact_values_for();
    top(construct!([f, d]), "search")
}

// --- H-S1: usage of a bound command ------------------------------------------

/// `source` whose `sync` is `sync_apply_opts()` transformed by `f`.
fn source_with_sync(
    f: impl FnOnce(OptionParser<SrcCmd>) -> OptionParser<SrcCmd>,
) -> OptionParser<Source> {
    source_r2(
        reserved_guard().boxed(),
        source_name(),
        bound_cmds_with(f(sync_apply_opts()).command("sync")),
    )
}

const SYNC_HELP_TAIL: &str = "\n\nAvailable options:\n        --apply  apply the plan\n    \
                              -h, --help   Prints help information\n";

#[test]
fn r2_hs1_with_usage_receives_only_the_args_fragment() {
    // No `Usage: `, no command path: just this parser's own items.
    assert_eq!(usage_of(sync_apply_opts()), "[--apply]");
    // bpaf's derived line: `Usage: ` + path + fragment — the path drops
    // the curried `SOURCE-NAME` (as S5 showed).
    assert_eq!(
        show(
            source_with_sync(|o| o),
            &["source", "bandcamp", "sync", "--help"]
        ),
        format!("STDOUT:\nsync a source\n\nUsage: source sync [--apply]{SYNC_HELP_TAIL}")
    );
}

#[test]
fn r2_hs1_a_custom_usage_replaces_the_whole_line_including_the_path() {
    let want = format!(
        "STDOUT:\nsync a source\n\nUsage: source SOURCE-NAME sync [--apply]{SYNC_HELP_TAIL}"
    );
    // A fixed string: replaces `Usage: ` *and* the path, verbatim.
    assert_eq!(
        show(
            source_with_sync(|o| o.usage("Usage: source SOURCE-NAME sync [--apply]")),
            &["source", "bandcamp", "sync", "--help"]
        ),
        want
    );
    // Composed: caller-supplied prefix (incl. `Usage: `) + bpaf's fragment.
    assert_eq!(
        show(
            source_with_sync(|o| o.with_usage(usage_prefixed("Usage: source SOURCE-NAME sync "))),
            &["source", "bandcamp", "sync", "--help"]
        ),
        want
    );
    // Errors do not quote the usage line, so they are unaffected.
    assert_eq!(
        show(
            source_with_sync(|o| o.with_usage(usage_prefixed("Usage: source SOURCE-NAME sync "))),
            &["source", "bandcamp", "sync", "--bogus"]
        ),
        "STDERR: `--bogus` is not expected in this context"
    );
}

#[test]
fn r2_hs1_curried_in_curried_usage_is_composable() {
    let p = {
        let b = positional::<String>("B");
        let f = pure(()).to_options().descr("f leaf").command("f");
        let inner = construct!(b, f).map(|(b, ())| b);
        let y = inner
            .to_options()
            .descr("y node")
            .with_usage(usage_prefixed("Usage: x A y "))
            .command("y");
        let a = positional::<String>("A");
        top(construct!(a, y), "x")
    };
    assert_eq!(
        show(p, &["x", "A", "y", "--help"]),
        "STDOUT:\ny node\n\nUsage: x A y B COMMAND ...\n\n\
         Available options:\n    -h, --help  Prints help information\n\n\
         Available commands:\n    f           f leaf\n"
    );
}

// --- H-S2: extracting a usage fragment ---------------------------------------

/// What `with_usage` hands its closure, rendered at width `w`.
fn usage_of_width<T>(p: OptionParser<T>, w: usize) -> String {
    let seen = RefCell::new(String::new());
    let _ = p.with_usage(|d| {
        *seen.borrow_mut() = format!("{d:w$}");
        d
    });
    seen.into_inner()
}

#[test]
fn r2_hs2_with_usage_extracts_the_fragment_without_running() {
    let branch = || {
        let g = reserved_guard();
        let n = source_name();
        let c = bound_cmds();
        construct!(g, n, c)
    };
    // `with_usage` capture and `--help` parsing agree on short fragments.
    assert_eq!(usage_of(source_name().to_options()), "SOURCE-NAME");
    assert_eq!(usage_via_help(source_name().to_options()), "SOURCE-NAME");
    assert_eq!(usage_of(branch().to_options()), "SOURCE-NAME COMMAND ...");
    assert_eq!(
        usage_via_help(branch().to_options()),
        "SOURCE-NAME COMMAND ..."
    );
    assert_eq!(
        usage_of(default_branch().to_options()),
        "[--artist=ARTIST] [--added=DAYS] [QUERY]"
    );
    assert_eq!(
        usage_via_help(default_branch().to_options()),
        "[--artist=ARTIST] [--added=DAYS] [QUERY]"
    );
    // The `MetaInfo` route renders the same fragment.
    let seen = std::rc::Rc::new(RefCell::new(String::new()));
    let sink = seen.clone();
    let p = source_name().with_group_help(move |mi: MetaInfo| {
        let mut d = Doc::default();
        d.meta(mi, true);
        *sink.borrow_mut() = d.to_string();
        d
    });
    let _ = p.to_options().with_usage(|d| d);
    assert_eq!(seen.borrow().as_str(), "SOURCE-NAME");
}

#[test]
fn r2_hs2_rendering_wraps_at_100_columns_unless_given_a_width() {
    let wide = || {
        let a = long("first-very-long-flag-name")
            .argument::<String>("FIRST-VALUE")
            .optional();
        let b = long("second-very-long-flag-name")
            .argument::<String>("SECOND-VALUE")
            .optional();
        let c = long("third-very-long-flag-name")
            .argument::<String>("THIRD-VALUE")
            .optional();
        let d = positional::<String>("FOURTH-POSITIONAL-ITEM");
        construct!(a, b, c, d).to_options()
    };
    // `Doc: Display` wraps at bpaf's MAX_WIDTH (100), mid-bracket…
    assert_eq!(
        usage_of(wide()),
        "[--first-very-long-flag-name=FIRST-VALUE] [--second-very-long-flag-name=SECOND-VALUE] [\n\
         --third-very-long-flag-name=THIRD-VALUE] FOURTH-POSITIONAL-ITEM"
    );
    // …so `--help` line parsing loses the tail…
    assert_eq!(
        usage_via_help(wide()),
        "[--first-very-long-flag-name=FIRST-VALUE] [--second-very-long-flag-name=SECOND-VALUE] ["
    );
    // …while a `{:width$}` format renders it on one line.
    assert_eq!(
        usage_of_width(wide(), 10_000),
        "[--first-very-long-flag-name=FIRST-VALUE] [--second-very-long-flag-name=SECOND-VALUE] \
         [--third-very-long-flag-name=THIRD-VALUE] FOURTH-POSITIONAL-ITEM"
    );
}

// --- H-S3: a section of its own for the bound commands -----------------------

/// `source` with the curried branch decorated by `deco`.
fn source_decorated<P: Parser<Source> + 'static>(
    deco: impl FnOnce(Box<dyn Parser<Source>>) -> P,
) -> OptionParser<Source> {
    let g = reserved_guard();
    let n = source_name();
    let c = bound_cmds();
    let guarded = construct!(g, n, c)
        .map(|((), n, c)| Source::Bound(n, c))
        .boxed();
    let guarded = deco(guarded);
    let fixed = list();
    top(construct!([fixed, guarded]), "source")
}

const SOURCE_HELP_HEAD: &str = "\
STDOUT:
the node

Usage: source (COMMAND ... | SOURCE-NAME COMMAND ...)

";

const SOURCE_HELP_FOOT: &str = "\
Available options:
    -h, --help   Prints help information

Available commands:
    list         list sources
";

#[test]
fn r2_hs3_group_help_on_the_curried_branch_splits_the_command_list() {
    // The section is typed by its *first* item (SOURCE-NAME, a positional),
    // so the positional and its bound commands move together; `list` stays.
    assert_eq!(
        show(
            source_decorated(|b| b.group_help("Commands for SOURCE-NAME:")),
            &["source", "--help"]
        ),
        format!(
            "{SOURCE_HELP_HEAD}Commands for SOURCE-NAME:\n    SOURCE-NAME  the source\n    \
             sync         sync a source\n    status       show a source's status\n    \
             cancel       cancel a job\n\n{SOURCE_HELP_FOOT}"
        )
    );
}

#[test]
fn r2_hs3_group_help_on_the_bound_commands_alone_leaves_the_positional() {
    let p = source_r2(
        reserved_guard().boxed(),
        source_name(),
        bound_cmds().group_help("Commands for SOURCE-NAME:"),
    );
    assert_eq!(
        show(p, &["source", "--help"]),
        format!(
            "{SOURCE_HELP_HEAD}Commands for SOURCE-NAME:\n    sync         sync a source\n    \
             status       show a source's status\n    cancel       cancel a job\n\n\
             Available positional items:\n    SOURCE-NAME  the source\n\n{SOURCE_HELP_FOOT}"
        )
    );
}

#[test]
fn r2_hs3_with_group_help_keeps_only_the_first_text_token_of_the_header() {
    // Built from several tokens: the header is cut after the first.
    let multi = source_decorated(|b| {
        b.with_group_help(|mi: MetaInfo| {
            let mut d = Doc::default();
            d.text("Commands for ");
            d.meta(mi, true);
            d.text(":");
            d
        })
    });
    assert_eq!(
        show(multi, &["source", "--help"]),
        format!(
            "{SOURCE_HELP_HEAD}Commands for \n    SOURCE-NAME  the source\n    \
             sync         sync a source\n    status       show a source's status\n    \
             cancel       cancel a job\n\n{SOURCE_HELP_FOOT}"
        )
    );
    // Rendered to one string first, the whole header survives — and the
    // `MetaInfo` is the branch's usage fragment, not a name.
    let single = source_decorated(|b| {
        b.with_group_help(|mi: MetaInfo| {
            let mut m = Doc::default();
            m.meta(mi, true);
            Doc::from(format!("Commands for {m}:").as_str())
        })
    });
    assert_eq!(
        show(single, &["source", "--help"]),
        format!(
            "{SOURCE_HELP_HEAD}Commands for SOURCE-NAME COMMAND ...:\n    \
             SOURCE-NAME  the source\n    sync         sync a source\n    \
             status       show a source's status\n    cancel       cancel a job\n\n\
             {SOURCE_HELP_FOOT}"
        )
    );
}

// --- C-S1: completing a fixed sibling's prefix -------------------------------

const HIDES: [&str; 4] = ["none", "hide", "hide_usage", "custom_usage"];

/// `source` with the completing `any` guard, wrapped by `hide`.
fn source_any_guard(hide: &str) -> OptionParser<Source> {
    source_r2(
        completing_guard(RESERVED, SOURCE_FIXED, hide),
        source_name(),
        bound_cmds(),
    )
}

/// `source` with S4's guard and a `SOURCE-NAME` completer offering `fixed`.
fn source_name_completing(fixed: Fixed) -> OptionParser<Source> {
    source_r2(
        reserved_guard().boxed(),
        source_name_with(fixed),
        bound_cmds(),
    )
}

#[test]
fn r2_cs1_a_completer_on_any_is_never_invoked() {
    // `any` registers no completion slot, so `.complete` on it is inert —
    // whether its predicate accepts the word or rejects it.
    for accept_all in [true, false] {
        assert_eq!(comp(any_alone(accept_all), &["t", "l"]), "COMPLETION:\n");
        assert_eq!(comp(any_alone(accept_all), &["t", ""]), "COMPLETION:\n");
    }
    // As a guard, under every hiding wrapper: `source l` / `search f` still
    // complete to nothing (S6's payload, unchanged).
    for hide in HIDES {
        assert_eq!(
            comp(source_any_guard(hide), &["source", "l"]),
            "COMPLETION:\n\n"
        );
        let s = search_r2(
            Some(completing_guard(&["fact-values-for"], SEARCH_FIXED, hide)),
            Fixed::No,
        );
        assert_eq!(comp(s, &["search", "f"]), "COMPLETION:\n\n");
        // …and parsing is unaffected.
        assert_eq!(
            show(source_any_guard(hide), &["source", "list"]),
            "Ok(List)"
        );
    }
}

#[test]
fn r2_cs1_the_positional_completer_can_offer_the_sibling_names() {
    // bpaf keeps only the branch that consumed the typed word, so the
    // sibling's name must come from that branch: SOURCE-NAME's completer.
    let p = || source_name_completing(Fixed::NonEmpty);
    assert_eq!(comp(p(), &["source", "l"]), "COMPLETION:\nlist\n");
    assert_eq!(
        comp(p(), &["source", "b"]),
        "COMPLETION:\n\tthe source\nbandcamp\nbandit\n"
    );
    assert_eq!(
        comp(p(), &["source", "bandcamp", ""]),
        "COMPLETION:\nsync\tsync a source\nstatus\tshow a source's status\ncancel\tcancel a job\n"
    );
    // With nothing typed both branches survive and the command itself is
    // offered — which is why the completer adds names only after a prefix.
    let empty = "COMPLETION:\nlist\tlist sources\n\tthe source\nbandcamp\nbandit\ndiscogs\n\
                 sync\tsync a source\nstatus\tshow a source's status\ncancel\tcancel a job\n";
    assert_eq!(comp(p(), &["source", ""]), empty);
    // Offering them always duplicates `list` on an empty word.
    assert_eq!(
        comp(source_name_completing(Fixed::Always), &["source", ""]),
        "COMPLETION:\nlist\tlist sources\n\tthe source\nbandcamp\nbandit\ndiscogs\n\
         list\tlist sources\nsync\tsync a source\nstatus\tshow a source's status\n\
         cancel\tcancel a job\n"
    );
    assert_eq!(
        comp(source_name_completing(Fixed::Always), &["source", "l"]),
        "COMPLETION:\nlist\n"
    );

    // The same recipe on `search`'s QUERY.
    let s = || search_r2(None, Fixed::NonEmpty);
    assert_eq!(
        comp(s(), &["search", "f"]),
        "COMPLETION:\nfact-values-for\n"
    );
    assert_eq!(comp(s(), &["search", "j"]), "COMPLETION:\n\n");
    assert_eq!(
        comp(s(), &["search", ""]),
        "COMPLETION:\nfact-values-for\tlist values for a fact type\n\
         --artist\tby artist\n--added\n\tfree text\n"
    );
    // Always: the duplicate, and QUERY's own hint line is lost (a completer
    // returning exactly one value strips the metavar).
    assert_eq!(
        comp(search_r2(None, Fixed::Always), &["search", ""]),
        "COMPLETION:\nfact-values-for\tlist values for a fact type\n\
         --artist\tby artist\n--added\nfact-values-for\tlist values for a fact type\n"
    );
}

// --- C-S2: the guard in help -------------------------------------------------

#[test]
fn r2_cs2_only_hide_and_hide_usage_keep_the_any_guard_out_of_help() {
    let guardless = format!("STDOUT:\n{SOURCE_HELP}");
    assert_eq!(
        show(source_any_guard("hide"), &["source", "--help"]),
        guardless
    );
    assert_eq!(
        show(source_any_guard("hide_usage"), &["source", "--help"]),
        guardless
    );
    // Unwrapped: `[]` in the usage line. `custom_usage("")`: a double space.
    assert_eq!(
        show(source_any_guard("none"), &["source", "--help"]),
        guardless.replace("| SOURCE-NAME", "| [] SOURCE-NAME")
    );
    assert_eq!(
        show(source_any_guard("custom_usage"), &["source", "--help"]),
        guardless.replace("| SOURCE-NAME", "|  SOURCE-NAME")
    );
}

// --- C-S3: bound commands offered at the SOURCE-NAME slot ---------------------

const EMPTY_SLOT: &str = "COMPLETION:\nlist\tlist sources\n\tthe source\nbandcamp\nbandit\n\
                          discogs\nsync\tsync a source\nstatus\tshow a source's status\n\
                          cancel\tcancel a job\n";

#[test]
fn r2_cs3_no_combinator_keeps_bound_commands_out_of_the_name_slot() {
    // (a) a completer returning values: round 1's node already had one.
    assert_eq!(
        comp(source_name_completing(Fixed::NonEmpty), &["source", ""]),
        EMPTY_SLOT
    );
    // (b) `construct!(t, bound_alt).adjacent()`.
    let adj = {
        let g = reserved_guard();
        let n = source_name_with(Fixed::NonEmpty);
        let c = bound_cmds();
        let b = construct!(n, c)
            .adjacent()
            .map(|(n, c)| Source::Bound(n, c));
        let guarded = construct!(g, b).map(|((), s)| s);
        let fixed = list();
        top(construct!([fixed, guarded]), "source")
    };
    assert_eq!(comp(adj, &["source", ""]), EMPTY_SLOT);
    // (c) `bound_alt.optional()` + an error when it is missing.
    let opt = || {
        let g = reserved_guard();
        let n = source_name_with(Fixed::NonEmpty);
        let c = bound_cmds().optional();
        let guarded = construct!(g, n, c).parse(|((), n, c)| {
            c.map(|c| Source::Bound(n, c))
                .ok_or("expected a command after SOURCE-NAME")
        });
        let fixed = list();
        top(construct!([fixed, guarded]), "source")
    };
    assert_eq!(comp(opt(), &["source", ""]), EMPTY_SLOT);
    assert_eq!(comp(opt(), &["source", "l"]), "COMPLETION:\nlist\n");
    assert_eq!(
        comp(opt(), &["source", "bandcamp", ""]),
        "COMPLETION:\nsync\tsync a source\nstatus\tshow a source's status\ncancel\tcancel a job\n"
    );
    assert_eq!(
        show(opt(), &["source", "bandcamp"]),
        "STDERR: couldn't parse: expected a command after SOURCE-NAME"
    );
    // (d) `.hide()` is bpaf's only comp-dropping wrapper: it drops them at
    // every position, and drops the commands from help too.
    let hidden = || {
        source_r2(
            reserved_guard().boxed(),
            source_name_with(Fixed::NonEmpty),
            bound_cmds().hide(),
        )
    };
    assert_eq!(
        comp(hidden(), &["source", ""]),
        "COMPLETION:\nlist\tlist sources\n\tthe source\nbandcamp\nbandit\ndiscogs\n"
    );
    assert_eq!(comp(hidden(), &["source", "bandcamp", ""]), "COMPLETION:\n");
    assert_eq!(
        show(hidden(), &["source", "--help"]),
        "STDOUT:\nthe node\n\nUsage: source (COMMAND ... | SOURCE-NAME)\n\n\
         Available positional items:\n    SOURCE-NAME  the source\n\n\
         Available options:\n    -h, --help   Prints help information\n\n\
         Available commands:\n    list         list sources\n"
    );
}

// --- C-S4: the shell revisions -----------------------------------------------

const ZSH_EMPTY_SLOT: &str = r#"COMPLETION:
local -a descr
descr=('list                     -- list sources')
compadd -l -V nosort -d descr -- 'list'
descr=('SOURCE-NAME: the source')
compadd -l -V nosort -d descr -- ''
descr=('bandcamp')
compadd -l -V nosort -d descr -- 'bandcamp'
descr=('bandit')
compadd -l -V nosort -d descr -- 'bandit'
descr=('discogs')
compadd -l -V nosort -d descr -- 'discogs'
descr=('sync                     -- sync a source')
compadd -l -V nosort -d descr -- 'sync'
descr=('status                   -- show a source'\''s status')
compadd -l -V nosort -d descr -- 'status'
descr=('cancel                   -- cancel a job')
compadd -l -V nosort -d descr -- 'cancel'
"#;

const BASH_EMPTY_SLOT: &str = r#"COMPLETION:
COMPREPLY+=('list                     -- list sources')
COMPREPLY+=('SOURCE-NAME: the source')
COMPREPLY+=('bandcamp')
COMPREPLY+=('bandit')
COMPREPLY+=('discogs')
COMPREPLY+=('sync                     -- sync a source')
COMPREPLY+=('status                   -- show a source'\''s status')
COMPREPLY+=('cancel                   -- cancel a job')
"#;

const ZSH_BOUND: &str = r#"COMPLETION:
local -a descr
descr=('sync                     -- sync a source')
compadd -l -V nosort -d descr -- 'sync'
descr=('status                   -- show a source'\''s status')
compadd -l -V nosort -d descr -- 'status'
descr=('cancel                   -- cancel a job')
compadd -l -V nosort -d descr -- 'cancel'
"#;

const BASH_BOUND: &str = r#"COMPLETION:
COMPREPLY+=('sync                     -- sync a source')
COMPREPLY+=('status                   -- show a source'\''s status')
COMPREPLY+=('cancel                   -- cancel a job')
"#;

#[test]
fn r2_cs4_the_name_completer_recipe_holds_at_every_revision() {
    let p = || source_name_completing(Fixed::NonEmpty);
    let s = || search_r2(None, Fixed::NonEmpty);

    // `source l` → `list`
    assert_eq!(comp_rev(1, p(), &["source", "l"]), "COMPLETION:\nlist\n");
    assert_eq!(
        comp_rev(7, p(), &["source", "l"]),
        "COMPLETION:\ncompadd -- 'list'\n"
    );
    assert_eq!(
        comp_rev(8, p(), &["source", "l"]),
        "COMPLETION:\nCOMPREPLY+=( 'list' )\n\n"
    );
    assert_eq!(
        comp_rev(9, p(), &["source", "l"]),
        "COMPLETION:\nlist\tlist sources\n"
    );

    // `search f` → `fact-values-for`
    assert_eq!(
        comp_rev(1, s(), &["search", "f"]),
        "COMPLETION:\nfact-values-for\n"
    );
    assert_eq!(
        comp_rev(7, s(), &["search", "f"]),
        "COMPLETION:\ncompadd -- 'fact-values-for'\n"
    );
    assert_eq!(
        comp_rev(8, s(), &["search", "f"]),
        "COMPLETION:\nCOMPREPLY+=( 'fact-values-for' )\n\n"
    );
    assert_eq!(
        comp_rev(9, s(), &["search", "f"]),
        "COMPLETION:\nfact-values-for\tlist values for a fact type\n"
    );

    // `source ""` — C-S3's leak is present at every revision.
    assert_eq!(comp_rev(1, p(), &["source", ""]), EMPTY_SLOT);
    assert_eq!(comp_rev(7, p(), &["source", ""]), ZSH_EMPTY_SLOT);
    assert_eq!(comp_rev(8, p(), &["source", ""]), BASH_EMPTY_SLOT);
    assert_eq!(
        comp_rev(9, p(), &["source", ""]),
        "COMPLETION:\ncancel\tcancel a job\nstatus\tshow a source's status\nsync\tsync a source\n\
         discogs\nbandit\nbandcamp\nlist\tlist sources\n"
    );

    // `source bandcamp ""` — the bound commands, correctly.
    assert_eq!(
        comp_rev(1, p(), &["source", "bandcamp", ""]),
        "COMPLETION:\nsync\tsync a source\nstatus\tshow a source's status\ncancel\tcancel a job\n"
    );
    assert_eq!(comp_rev(7, p(), &["source", "bandcamp", ""]), ZSH_BOUND);
    assert_eq!(comp_rev(8, p(), &["source", "bandcamp", ""]), BASH_BOUND);
    assert_eq!(
        comp_rev(9, p(), &["source", "bandcamp", ""]),
        "COMPLETION:\ncancel\tcancel a job\nstatus\tshow a source's status\nsync\tsync a source\n"
    );
}

#[test]
fn r2_cs4_without_the_recipe_bash_and_zsh_insert_the_metavar_literally() {
    // The any-guard node (its completer inert, so: round 1's completion).
    // Revision 1 and fish show nothing; zsh and bash offer `SOURCE-NAME`
    // itself as the single candidate.
    let p = || source_any_guard("hide_usage");
    assert_eq!(comp_rev(1, p(), &["source", "l"]), "COMPLETION:\n\n");
    assert_eq!(
        comp_rev(7, p(), &["source", "l"]),
        "COMPLETION:\ncompadd -- 'SOURCE-NAME'\ncompadd ''\n"
    );
    assert_eq!(
        comp_rev(8, p(), &["source", "l"]),
        "COMPLETION:\nCOMPREPLY+=( 'SOURCE-NAME' '')\n"
    );
    assert_eq!(comp_rev(9, p(), &["source", "l"]), "COMPLETION:\n");
}

// --- C-S5: errors with the guard ---------------------------------------------

#[test]
fn r2_cs5_errors_are_unchanged_by_the_guard_and_the_recipe() {
    let blame =
        "STDERR: expected `COMMAND ...`, got `bandcamp`. Pass `--help` for usage information";
    let lst = "STDERR: no such command or positional: `lst`, did you mean `list`?";
    for p in [
        || source_name_completing(Fixed::NonEmpty),
        || source_any_guard("hide_usage"),
    ] {
        assert_eq!(show(p(), &["source", "bandcamp", "snyc"]), blame);
        assert_eq!(show(p(), &["source", "lst"]), lst);
    }
    assert_eq!(
        show(
            search_r2(None, Fixed::NonEmpty),
            &["search", "fact-valus-for"]
        ),
        r#"Ok(Default { query: Some("fact-valus-for"), artist: None, added: None })"#
    );
}
