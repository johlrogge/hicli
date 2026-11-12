//! `Option<T>` as a command's parameter means "T, if given" (ADR-019 §3): `T`'s
//! parser made optional, `T` keeping its own `FromStr`, help, completion and
//! named errors. It compiles only when `T` is required — everything not marked
//! optional (`Arg::OPTIONAL`) is required, and the derive marks it.

use std::cell::RefCell;
use std::num::NonZeroUsize;
use std::str::FromStr;

use hicli::bpaf::{pure, Bpaf, Parser};
use hicli::{app, cli, command, exit_code_for, group, App, Arg, Candidate, Choice};

/// Records what each command was handed.
#[derive(Default)]
pub struct Ctx {
    seen: RefCell<Vec<String>>,
}

impl Ctx {
    fn saw(&self, s: String) {
        self.seen.borrow_mut().push(s);
    }
}

/// How many to take
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arg)]
#[arg(long = "limit", metavar = "N")]
pub struct Limit(pub NonZeroUsize);

impl FromStr for Limit {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Limit).map_err(|_| "must be at least 1")
    }
}

/// A track id
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(positional = "ID")]
pub struct Id(pub String);

/// What to sort by
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice, Arg)]
#[arg(long = "by", metavar = "KEY")]
pub enum SortKey {
    /// Beats per minute
    Bpm,
    /// Track title
    Title,
}

/// A source on the node
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(long = "source", metavar = "SOURCE", complete = complete_sources)]
pub struct Source(pub String);

/// The word typed so far, completed against two sources.
fn complete_sources(typed: &str) -> Vec<Candidate> {
    ["bandcamp", "beatport"]
        .into_iter()
        .filter(|s| s.starts_with(typed))
        .map(|s| (s.to_string(), None))
        .collect()
}

/// A port, written by hand, with no `OPTIONAL` line: required by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(pub u16);

impl Arg for Port {
    fn parser() -> impl Parser<Self> {
        hicli::bpaf::long("port")
            .help("The port")
            .argument::<u16>("PORT")
            .map(Port)
    }
}

/// A title spelled by `#[derive(Bpaf)]`: required by default.
#[derive(Debug, Clone, PartialEq, Eq, Bpaf, Arg)]
pub struct Title(#[bpaf(positional("TITLE"))] pub String);

/// List tracks
#[command]
pub fn list(limit: Option<Limit>, ctx: &Ctx) {
    ctx.saw(format!("list {limit:?}"));
}

/// Show tracks
///
/// # Arguments
///
/// * `limit` - Show at most this many
#[command]
pub fn show(limit: Option<Limit>, ctx: &Ctx) {
    ctx.saw(format!("show {limit:?}"));
}

/// Play a track
#[command]
pub fn play(by: Option<SortKey>, id: Option<Id>, ctx: &Ctx) {
    ctx.saw(format!("play {by:?} {id:?}"));
}

/// Sync from a source
#[command]
pub fn sync(source: Option<Source>, port: Option<Port>, ctx: &Ctx) {
    ctx.saw(format!("sync {source:?} {port:?}"));
}

/// Tag a track
#[command]
pub fn tag(title: Option<Title>, ctx: &Ctx) {
    ctx.saw(format!("tag {title:?}"));
}

/// The event directory
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(positional = "DIR")]
pub struct Dir(pub String);

/// The instance
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(positional = "INSTANCE")]
pub struct Instance(pub String);

/// An event, by directory and instance: both required.
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
pub struct EventRef {
    /// The event directory
    pub dir: Dir,
    /// The instance
    pub instance: Instance,
}

/// Open an event
#[command]
pub fn open(r: Option<EventRef>, ctx: &Ctx) {
    ctx.saw(format!("open {r:?}"));
}

/// Search the library
#[group]
pub mod search {
    use super::*;

    /// Search by id
    #[command(default)]
    pub fn by_id(id: Option<Id>, ctx: &Ctx) {
        ctx.saw(format!("search {id:?}"));
    }
}

/// A source's name
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(positional = "NAME")]
pub struct SourceName(pub String);

/// Music sources
#[group(args = SourceName)]
pub mod source {
    use super::*;

    /// Check the source
    #[command(curried)]
    pub fn check(s: SourceName, limit: Option<Limit>, ctx: &Ctx) {
        ctx.saw(format!("check {} {limit:?}", s.0));
    }
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [list, show, play, sync, tag, open, search, source] }
}

/// What running `args` handed its command.
fn ran(args: Vec<String>) -> String {
    let (_, inv) = app().parse(args).expect("parses");
    let ctx = Ctx::default();
    inv.run(&ctx).expect("runs");
    let seen = ctx.seen.borrow();
    seen.join("\n")
}

/// `args`' help page.
fn help_of(args: Vec<String>) -> String {
    let e = app().parse(args).err().expect("--help short-circuits");
    assert_eq!(exit_code_for(&e), 0);
    e.unwrap_stdout()
}

/// `args`' usage error.
fn error_of(args: Vec<String>) -> String {
    let e = app().parse(args).err().expect("a usage error");
    assert_eq!(exit_code_for(&e), 2);
    e.unwrap_stderr()
}

fn limit_of(n: usize) -> Limit {
    Limit(NonZeroUsize::new(n).expect("not zero"))
}

#[test]
fn absent_is_none_and_present_is_some() {
    assert_eq!(ran(cli! { list }), "list None");
    assert_eq!(
        ran(cli! { list --limit 3 }),
        format!("list {:?}", Some(limit_of(3)))
    );
}

#[test]
fn the_types_own_rule_still_runs() {
    assert_eq!(
        error_of(cli! { list --limit 0 }),
        "couldn't parse `0`: `--limit`: must be at least 1"
    );
}

#[test]
fn the_usage_shows_it_optional() {
    assert_eq!(
        help_of(cli! { list --help }),
        "List tracks\n\
         \n\
         Usage: list [--limit=N]\n\
         \n\
         Available options:\n\
         \x20       --limit=N  How many to take\n\
         \x20   -h, --help     Prints help information\n"
    );
}

#[test]
fn an_arguments_entry_replaces_the_row_and_it_stays_optional() {
    assert_eq!(
        help_of(cli! { show --help }),
        "Show tracks\n\
         \n\
         Usage: show [--limit=N]\n\
         \n\
         Available options:\n\
         \x20       --limit=N  Show at most this many\n\
         \x20   -h, --help     Prints help information\n"
    );
    assert_eq!(ran(cli! { show }), "show None");
}

#[test]
fn an_optional_positional_and_an_optional_choice() {
    assert_eq!(
        help_of(cli! { play --help }),
        "Play a track\n\
         \n\
         Usage: play [--by=KEY] [ID]\n\
         \n\
         Available positional items:\n\
         \x20   ID            A track id\n\
         \n\
         Available options:\n\
         \x20       --by=KEY  What to sort by [bpm, title]\n\
         \x20   -h, --help    Prints help information\n"
    );
    assert_eq!(ran(cli! { play }), "play None None");
    assert_eq!(
        ran(cli! { play --by bpm t-1 }),
        format!("play {:?} {:?}", Some(SortKey::Bpm), Some(Id("t-1".into())))
    );
    assert_eq!(app().complete(cli! { play --by "" }), ["bpm", "title"]);
    assert_eq!(
        error_of(cli! { play --by bmp }),
        "couldn't parse `bmp`: `--by`: expected `bpm` or `title`; did you mean `bpm`?"
    );
}

#[test]
fn an_optional_type_with_a_completer_completes_on_the_empty_word() {
    assert_eq!(
        app().complete(cli! { sync --source "" }),
        ["bandcamp", "beatport"]
    );
}

#[test]
fn a_hand_written_arg_without_an_optional_line_is_required() {
    const { assert!(!<Port as Arg>::OPTIONAL) };
    assert_eq!(ran(cli! { sync }), "sync None None");
    assert_eq!(
        ran(cli! { sync --port 7700 }),
        format!("sync None {:?}", Some(Port(7700)))
    );
}

#[test]
fn a_bpaf_mode_type_is_required_by_default() {
    const { assert!(!<Title as Arg>::OPTIONAL) };
    assert_eq!(ran(cli! { tag }), "tag None");
    assert_eq!(
        ran(cli! { tag "Summer" }),
        format!("tag {:?}", Some(Title("Summer".into())))
    );
}

#[test]
fn a_default_command_runs_with_none() {
    assert_eq!(ran(cli! { search }), "search None");
    assert_eq!(
        ran(cli! { search t-1 }),
        format!("search {:?}", Some(Id("t-1".into())))
    );
}

#[test]
fn a_curried_command_gets_its_value_and_an_optional_one() {
    assert_eq!(ran(cli! { source bandcamp check }), "check bandcamp None");
    assert_eq!(
        ran(cli! { source bandcamp check --limit 2 }),
        format!("check bandcamp {:?}", Some(limit_of(2)))
    );
}

#[test]
fn the_function_is_still_called_directly_with_none() {
    let ctx = Ctx::default();
    list(None, &ctx);
    list(Some(limit_of(1)), &ctx);
    assert_eq!(
        ctx.seen.borrow().join("\n"),
        format!("list None\nlist {:?}", Some(limit_of(1)))
    );
}

#[test]
fn an_optional_composite_of_required_fields_is_all_or_nothing() {
    assert_eq!(ran(cli! { open }), "open None");
    assert_eq!(
        ran(cli! { open ./ev main }),
        format!(
            "open {:?}",
            Some(EventRef {
                dir: Dir("./ev".into()),
                instance: Instance("main".into())
            })
        )
    );
    // Half given is an error, not `None`.
    assert_eq!(
        error_of(cli! { open ./ev }),
        "expected `INSTANCE`, pass `--help` for usage information"
    );
}

#[test]
fn the_derive_marks_what_succeeds_without_its_argument() {
    /// x
    #[derive(Debug, Clone, Arg)]
    #[arg(long = "a", metavar = "A")]
    struct Opt(#[allow(dead_code)] Option<String>);
    /// x
    #[derive(Debug, Clone, Arg)]
    #[arg(positional = "A")]
    struct Many(#[allow(dead_code)] Vec<String>);
    /// x
    #[derive(Debug, Clone, Arg)]
    #[arg(long = "a")]
    struct Switch(#[allow(dead_code)] bool);
    // A composite is optional exactly when every field is.
    #[derive(Debug, Clone, Arg)]
    struct Composite {
        #[allow(dead_code)]
        l: Limit,
    }
    #[derive(Debug, Clone, Arg)]
    struct AllOptional {
        #[allow(dead_code)]
        o: Opt,
        #[allow(dead_code)]
        s: Switch,
        #[allow(dead_code)]
        l: Option<Limit>,
    }
    #[derive(Debug, Clone, Arg)]
    struct Mixed {
        #[allow(dead_code)]
        o: Opt,
        #[allow(dead_code)]
        l: Limit,
    }
    const {
        assert!(<Opt as Arg>::OPTIONAL);
        assert!(<Many as Arg>::OPTIONAL);
        assert!(<Switch as Arg>::OPTIONAL);
        assert!(!<Composite as Arg>::OPTIONAL);
        assert!(<AllOptional as Arg>::OPTIONAL);
        assert!(!<Mixed as Arg>::OPTIONAL);
        assert!(!<Limit as Arg>::OPTIONAL);
        assert!(!<Id as Arg>::OPTIONAL);
        assert!(!<SortKey as Arg>::OPTIONAL);
    }
}
