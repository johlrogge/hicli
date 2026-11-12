//! `playr`, the README's running example: every `playr` Rust block in the
//! README is an excerpt of this file, and every console block is its output.
//!
//! Try:
//! ```text
//! cargo run --example playr -- --help
//! cargo run --example playr -- search --added=-7 --artist CBL techno
//! cargo run --example playr -- source bandcamp check-updates --apply
//! cargo test --example playr
//! ```

// No struct here is spelled by `#[derive(Bpaf)]`, and playr has no `bpaf`
// dependency. `Filters` is a struct of arguments: a doc comment on it would be
// a help heading, and a doc comment on a field would replace that field's help,
// so it has neither (see the README). The program's other public items are
// example code.
#![allow(missing_docs)]

use std::fmt;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::OnceLock;

use hicli::{app, command, group, App, Arg, Candidate, Choice, Failure, Outcome};

// ---- globals and context -------------------------------------------------

/// The node to talk to
#[derive(Debug, Clone, Arg)]
#[arg(long = "node", metavar = "HOST", env = "PLAYR_NODE")]
pub struct Globals(pub Option<Node>);

/// Built once, after parsing, and handed to every command.
pub struct Ctx {
    pub node: Node,
    session: OnceLock<Session>,
}

impl Ctx {
    /// The session with the node, opened the first time a command asks.
    pub fn session(&self) -> Result<&Session, Failure> {
        // `OnceLock::get_or_try_init` is unstable.
        if let Some(session) = self.session.get() {
            return Ok(session);
        }
        let opened = Session::open(&self.node)?;
        Ok(self.session.get_or_init(|| opened))
    }
}

fn mk_ctx(g: Globals) -> Result<Ctx, Failure> {
    Ok(Ctx {
        node: g.0.unwrap_or_else(Node::localhost),
        session: OnceLock::new(),
    })
}

/// A connection to the node: costly to open, so only a command that talks to
/// the node opens it.
pub struct Session {
    pub node: Node,
}

impl Session {
    /// A real client connects here, and fails when nothing answers.
    fn open(node: &Node) -> Result<Session, Failure> {
        Ok(Session { node: node.clone() })
    }
}

// ---- domain types ---------------------------------------------------------

/// Lowercase letters, digits and `-`, such as `hdmi` or `bandcamp`.
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);

impl FromStr for Name {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if s.is_empty() || !s.chars().all(allowed) {
            return Err("a name uses only a-z, 0-9 and -".to_string());
        }
        Ok(Name(s.to_string()))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A host with an optional port: `pi.local` or `pi.local:7700`.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub host: String,
    pub port: Option<u16>,
}

impl Node {
    pub fn localhost() -> Self {
        Node {
            host: "localhost".to_string(),
            port: None,
        }
    }
}

impl FromStr for Node {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let (host, port) = match s.split_once(':') {
            Some((host, port)) => match port.parse() {
                Ok(port) => (host, Some(port)),
                Err(_) => return Err(format!("`{port}` is not a port number")),
            },
            None => (s, None),
        };
        let allowed = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.';
        if host.is_empty() || !host.chars().all(allowed) {
            return Err(format!("`{host}` is not a host name"));
        }
        Ok(Node {
            host: host.to_string(),
            port,
        })
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.port {
            Some(port) => write!(f, "{}:{port}", self.host),
            None => f.write_str(&self.host),
        }
    }
}

/// A tempo in beats per minute, from 20 to 300.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bpm(u16);

impl FromStr for Bpm {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.parse() {
            Ok(bpm @ 20..=300) => Ok(Bpm(bpm)),
            _ => Err("a tempo is from 20 to 300".to_string()),
        }
    }
}

/// How many days back: `-7` is the last seven days.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DaysBack(NonZeroU32);

impl FromStr for DaysBack {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.strip_prefix('-').map(str::parse) {
            Some(Ok(days)) => Ok(DaysBack(days)),
            _ => Err("not a number of days back, such as -7".to_string()),
        }
    }
}

// ---- arguments ------------------------------------------------------------
//
// Spelled by `#[arg(…)]` itself, with no `#[derive(Bpaf)]`: the doc comment's
// first paragraph is the help row. `Filters`, which has several fields, is a
// struct of such types, composed by `#[derive(Arg)]`.

/// An audio output device
#[derive(Debug, Clone, Arg)]
#[arg(positional = "NAME")]
pub struct Output(pub Name);

/// Free-text query over title, artist and album
#[derive(Debug, Clone, Arg)]
#[arg(positional = "QUERY")]
pub struct Query(pub Option<String>);

/// Only tracks by this artist
#[derive(Debug, Clone, Arg)]
#[arg(long = "artist", metavar = "NAME")]
pub struct Artist(pub Option<String>);

/// Only tracks at this tempo
#[derive(Debug, Clone, Arg)]
#[arg(long = "bpm", metavar = "BPM")]
pub struct BpmFilter(pub Option<Bpm>);

/// Only tracks that were never played
#[derive(Debug, Clone, Arg)]
#[arg(long = "unplayed")]
pub struct Unplayed(pub bool);

#[derive(Debug, Clone, Arg)]
pub struct Filters {
    pub artist: Artist,
    pub bpm: BpmFilter,
    pub unplayed: Unplayed,
}

/// A fact type, such as `genre` or `label`
#[derive(Debug, Clone, Arg)]
#[arg(positional = "TYPE")]
pub struct FactType(pub Name);

/// Only tracks added in the last DAYS days, e.g. -7
#[derive(Debug, Clone, Arg)]
#[arg(long = "added", metavar = "DAYS")]
pub struct Added(pub Option<DaysBack>);

/// Do not ask for confirmation
#[derive(Debug, Clone, Arg)]
#[arg(short = 'y', long = "yes")]
pub struct Yes(pub bool);

/// A source on the node: bandcamp, beatport or upload
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SOURCE", complete = complete_sources)]
pub struct SourceName(pub Name);

/// Runs on every TAB, with the word typed so far: never fails, and filters.
fn complete_sources(typed: &str) -> Vec<Candidate> {
    ["bandcamp", "beatport", "upload"]
        .into_iter()
        .filter(|s| s.starts_with(typed))
        .map(|s| (s.to_string(), None))
        .collect()
}

/// Re-download anything that is out of date
#[derive(Debug, Clone, Arg)]
#[arg(long = "apply")]
pub struct Apply(pub bool);

/// An audio file to upload
#[derive(Debug, Clone, Arg)]
#[arg(positional = "FILE", complete = files)]
pub struct AudioFile(pub PathBuf);

/// What to sort by
//
// A choice is the argument: its names parse, complete and appear in errors.
#[derive(Debug, Clone, Copy, PartialEq, Choice, Arg)]
#[arg(positional = "KEY")]
pub enum SortKey {
    /// Beats per minute
    Bpm,
    /// Track title
    Title,
    /// Artist name
    Artist,
}

/// Sort descending (Z→A, high→low)
#[derive(Debug, Clone, Arg)]
#[arg(short = 'd', long = "descending")]
pub struct Descending(pub bool);

/// A shell to print completions for
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SHELL")]
pub struct ShellArg(pub hicli::Shell);

// ---- commands -------------------------------------------------------------

/// Check that the node answers
#[command]
fn ping(ctx: &Ctx) -> Outcome {
    let session = ctx.session()?;
    println!("pong from {}", session.node);
    Ok(())
}

/// Playback control
#[group]
mod playback {
    use super::*;

    /// Start playback from the queue
    #[command]
    pub fn start(ctx: &Ctx) {
        println!("{}: playing", ctx.node);
    }

    /// Stop playback
    #[command]
    pub fn stop(ctx: &Ctx) {
        println!("{}: stopped", ctx.node);
    }

    /// Select an audio output device
    #[command]
    pub fn set_output(output: Output, ctx: &Ctx) {
        println!("{}: output is now {}", ctx.node, output.0);
    }
}

/// Search the library
#[group]
mod search {
    use super::*;

    /// List every value stored for a fact type
    #[command]
    pub fn fact_values_for(t: FactType, ctx: &Ctx) {
        println!("{}: values for {}", ctx.node, t.0);
    }

    /// Search by text and filters
    #[command(default)]
    pub fn tracks(added: Added, f: Filters, q: Query, ctx: &Ctx) {
        println!(
            "{}: search {:?} artist={:?} bpm={:?} unplayed={} added={:?}",
            ctx.node, q.0, f.artist.0, f.bpm.0, f.unplayed.0, added.0
        );
    }
}

/// Library maintenance
#[group]
mod library {
    use super::*;

    /// Files that duplicate merges moved aside
    #[group]
    pub mod archive {
        use super::*;

        /// Permanently delete everything in the archive
        ///
        /// # Arguments
        ///
        /// * `yes` - Confirm: the files cannot be recovered
        #[command]
        pub fn empty(yes: Yes, ctx: &Ctx) -> Outcome {
            if !yes.0 {
                return Err(Failure::usage(
                    "this cannot be undone: pass --yes to confirm",
                ));
            }
            println!("{}: archive emptied", ctx.node);
            Ok(())
        }

        /// List the archive
        #[command(default)]
        pub fn list(ctx: &Ctx) {
            println!("{}: 3 files in the archive", ctx.node);
        }
    }
}

/// Music sources
#[group(args = SourceName)]
mod source {
    use super::*;

    /// List the sources on the node
    #[command]
    pub fn list(ctx: &Ctx) {
        println!("{}: bandcamp, beatport, upload", ctx.node);
    }

    /// Check the source's collection for stale items
    #[command(curried)]
    pub fn check_updates(s: SourceName, apply: Apply, ctx: &Ctx) {
        println!("{}: checking {} (apply: {})", ctx.node, s.0, apply.0);
    }
}

// Imported here, beside its only user, so the README's excerpt compiles as shown.
use color_eyre::eyre::WrapErr;

/// Upload an audio file to the library
#[command]
fn upload(file: AudioFile, ctx: &Ctx) -> color_eyre::Result<()> {
    let bytes = std::fs::read(&file.0)
        .wrap_err("reading the file")
        .wrap_err_with(|| format!("uploading {}", file.0.display()))?;
    println!("{}: uploaded {} bytes", ctx.node, bytes.len());
    Ok(())
}

/// Sort track ids read from stdin
///
/// Reads one track id per line and prints them sorted.
///
/// # Examples
///
/// ```sh
/// playr search --artist CBL | playr sort bpm --descending
/// ```
#[command]
fn sort(desc: Descending, by: SortKey) {
    println!("sorting by {by}, descending: {}", desc.0);
}

/// Print the shell completion script
#[command(hide)]
fn generate_completions(shell: ShellArg) {
    print!("{}", hicli::completion_script(shell.0, "playr"));
}

// ---- the app --------------------------------------------------------------

fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [ping, playback, search, library, source, upload, sort, generate_completions],
    }
    .about("playr: control the music player")
    .version(env!("CARGO_PKG_VERSION"))
}

fn main() -> ExitCode {
    app().run(mk_ctx)
}

// ---- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hicli::{cli, cli_test, exit_code_for};

    cli_test! {
        app: app();

        stop_is_under_playback: { playback stop } => |r| {
            let (_globals, invocation) = r.expect("parses");
            assert_eq!(invocation.path(), ["playback", "stop"]);
        },
        node_after_the_command: { playback stop --node "pi.local" } => |r| {
            let (globals, _) = r.expect("parses");
            assert_eq!(globals.0.map(|n| n.host).as_deref(), Some("pi.local"));
        },
        set_output_needs_a_name: { playback set-output } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("NAME"));
        },
        output_names_are_checked: { playback set-output HDMI } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert_eq!(
                e.unwrap_stderr(),
                "couldn't parse `HDMI`: `NAME`: a name uses only a-z, 0-9 and -"
            );
        },
        added_takes_the_glued_form: { search --added=-7 } => |r| {
            assert!(r.is_ok(), "--added=-7 parses");
        },
        added_rejects_a_separate_negative_word: { search --added -7 } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("try `--added=-7`"));
        },
        added_names_itself_in_errors: { search --added=-x } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(
                e.unwrap_stderr(),
                "couldn't parse `-x`: `--added`: not a number of days back, such as -7"
            );
        },
        node_names_itself_in_errors: { --node "pi.local:http" ping } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(
                e.unwrap_stderr(),
                "couldn't parse `pi.local:http`: `--node`: `http` is not a port number"
            );
        },
    }

    #[test]
    fn sort_never_opens_a_session() {
        let (globals, invocation) = app().parse(cli! { sort bpm }).expect("parses");
        let ctx = mk_ctx(globals).expect("building the context is cheap");
        invocation.run(&ctx).expect("sorts");
        assert!(ctx.session.get().is_none(), "sort opened a session");
    }

    #[test]
    fn ping_opens_the_session() {
        let (globals, invocation) = app().parse(cli! { ping }).expect("parses");
        let ctx = mk_ctx(globals).expect("building the context is cheap");
        assert!(ctx.session.get().is_none(), "mk_ctx opened a session");
        invocation.run(&ctx).expect("pongs");
        assert!(ctx.session.get().is_some(), "ping did not open a session");
    }

    #[test]
    fn emptying_the_archive_needs_yes() {
        assert_eq!(
            app().run_with_args(cli! { library archive empty }, mk_ctx),
            2
        );
        assert_eq!(
            app().run_with_args(cli! { library archive empty --yes }, mk_ctx),
            0
        );
    }

    /// `args`' help page as `playr` prints it.
    fn page(args: &[&str]) -> String {
        app()
            .bin_name("playr")
            .parse(args.iter().copied())
            .err()
            .expect("--help short-circuits")
            .unwrap_stdout()
    }

    // The README pastes these pages; these keep them true.

    #[test]
    fn top_help_matches_the_readme() {
        // The README shows `PLAYR_NODE` unset. A developer may have it
        // exported, and then help prints its value instead; this binary has
        // many tests, so read the variable rather than change it.
        let env = match std::env::var_os("PLAYR_NODE") {
            None => ": N/A".to_string(),
            Some(v) => format!(" = {:?}", v.to_string_lossy()),
        };
        assert_eq!(
            page(&["--help"]),
            format!(
                "\
playr: control the music player

Usage: playr [--node=HOST] COMMAND ...

Available options:
        --node=HOST  The node to talk to
                     [env:PLAYR_NODE{env}]
    -h, --help       Prints help information
    -V, --version    Prints version information

Available commands:
    ping             Check that the node answers
    playback         Playback control
    search           Search the library
    library          Library maintenance
    source           Music sources
    upload           Upload an audio file to the library
    sort             Sort track ids read from stdin
"
            )
        );
    }

    #[test]
    fn search_help_matches_the_readme() {
        assert_eq!(
            page(&["search", "--help"]),
            "\
Search the library

Usage: playr search (COMMAND ... | [--added=DAYS] [--artist=NAME] [--bpm=BPM] [--unplayed] [QUERY])

Search by text and filters
        --added=DAYS   Only tracks added in the last DAYS days, e.g. -7
        --artist=NAME  Only tracks by this artist
        --bpm=BPM      Only tracks at this tempo
        --unplayed     Only tracks that were never played
    QUERY              Free-text query over title, artist and album

Available options:
    -h, --help         Prints help information

Available commands:
    fact-values-for    List every value stored for a fact type
"
        );
    }

    #[test]
    fn library_archive_help_matches_the_readme() {
        assert_eq!(
            page(&["library", "archive", "--help"]),
            "\
Files that duplicate merges moved aside

Usage: playr library archive [COMMAND ...]

List the archive

Available options:
    -h, --help  Prints help information

Available commands:
    empty       Permanently delete everything in the archive
"
        );
    }

    #[test]
    fn source_help_matches_the_readme() {
        assert_eq!(
            page(&["source", "--help"]),
            "\
Music sources

Usage: playr source (COMMAND ... | SOURCE COMMAND ...)

Commands for SOURCE:
    check-updates  Check the source's collection for stale items

Available positional items:
    SOURCE         A source on the node: bandcamp, beatport or upload

Available options:
    -h, --help     Prints help information

Available commands:
    list           List the sources on the node
"
        );
    }

    #[test]
    fn a_source_without_a_command_matches_the_readme() {
        let e = app()
            .bin_name("playr")
            .parse(["source", "bandcamp"])
            .err()
            .expect("a usage error");
        assert_eq!(exit_code_for(&e), 2);
        assert_eq!(
            e.unwrap_stderr(),
            "expected a command after `bandcamp`: `check-updates`"
        );
    }

    #[test]
    fn sort_help_matches_the_readme() {
        assert_eq!(
            page(&["sort", "--help"]),
            "\
Sort track ids read from stdin

Usage: playr sort [-d] KEY

Reads one track id per line and prints them sorted.

Available positional items:
    KEY               What to sort by [bpm, title, artist]

Available options:
    -d, --descending  Sort descending (Z→A, high→low)
    -h, --help        Prints help information

Examples:
  playr search --artist CBL | playr sort bpm --descending
"
        );
    }

    #[test]
    fn sort_keys_complete_and_a_wrong_one_names_them() {
        // The empty word also offers the flag not given yet.
        assert_eq!(
            app().complete(cli! { sort "" }),
            ["--descending", "bpm", "title", "artist"]
        );
        assert_eq!(app().complete(cli! { sort t }), ["title"]);
        let e = app().parse(cli! { sort key }).err().expect("a usage error");
        assert_eq!(exit_code_for(&e), 2);
        assert_eq!(
            e.unwrap_stderr(),
            "couldn't parse `key`: `KEY`: expected `bpm`, `title` or `artist`"
        );
        let e = app().parse(cli! { sort bmp }).err().expect("a usage error");
        assert_eq!(
            e.unwrap_stderr(),
            "couldn't parse `bmp`: `KEY`: expected `bpm`, `title` or `artist`; did you mean `bpm`?"
        );
    }

    #[test]
    fn archive_empty_help_matches_the_readme() {
        assert_eq!(
            page(&["library", "archive", "empty", "--help"]),
            "\
Permanently delete everything in the archive

Usage: playr library archive empty [-y]

Available options:
    -y, --yes   Confirm: the files cannot be recovered
    -h, --help  Prints help information
"
        );
    }

    #[test]
    fn generate_completions_help_lists_the_shells() {
        assert_eq!(
            page(&["generate-completions", "--help"]),
            "\
Print the shell completion script

Usage: playr generate-completions SHELL

Available positional items:
    SHELL       A shell to print completions for [bash, zsh, fish]

Available options:
    -h, --help  Prints help information
"
        );
    }

    #[test]
    fn shells_complete() {
        assert_eq!(
            app().complete(cli! { generate-completions "" }),
            ["bash", "zsh", "fish"]
        );
    }

    #[test]
    fn source_names_complete() {
        assert_eq!(app().complete(cli! { source b }), ["bandcamp", "beatport"]);
        // The completer is handed the raw word, so it fires on the empty one.
        assert_eq!(
            app().complete(cli! { source "" }),
            ["list", "bandcamp", "beatport", "upload", "check-updates"]
        );
    }

    #[test]
    fn the_command_is_still_a_function() {
        let ctx = Ctx {
            node: Node::localhost(),
            session: OnceLock::new(),
        };
        assert!(upload(AudioFile("/no/such/file.flac".into()), &ctx).is_err());
    }
}
