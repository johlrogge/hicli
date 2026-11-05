//! `playr`, the README's running example: every Rust block in the README is an
//! excerpt of this file, and every console block is its output.
//!
//! Try:
//! ```text
//! cargo run --example playr -- --help
//! cargo run --example playr -- search --added=-7 --artist CBL techno
//! cargo run --example playr -- source bandcamp check-updates --apply
//! cargo test --example playr
//! ```

// The argument newtypes are documented on their fields, not the type: bpaf
// turns a doc comment on a struct into a help heading (see the README).
#![allow(missing_docs)]

use std::fmt;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use bpaf::Bpaf;
use hicli::{app, command, group, App, Arg, Candidate, Failure, Outcome};

// ---- globals and context -------------------------------------------------

/// Global options
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// The node to talk to
    #[bpaf(long, env("PLAYR_NODE"), argument("HOST"), fallback(Node::localhost()))]
    pub node: Node,
}

/// Built once, after parsing, and handed to every command.
pub struct Ctx {
    pub node: Node,
}

fn mk_ctx(g: Globals) -> Result<Ctx, Failure> {
    Ok(Ctx { node: g.node })
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
            _ => Err(format!("`{s}` is not a number of days back, such as -7")),
        }
    }
}

// ---- arguments ------------------------------------------------------------

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Output(
    /// An audio output device
    #[bpaf(positional("NAME"))]
    pub Name,
);

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Filters {
    /// Only tracks by this artist
    #[bpaf(long, argument("NAME"))]
    pub artist: Option<String>,
    /// Only tracks at this tempo
    #[bpaf(long, argument("BPM"))]
    pub bpm: Option<Bpm>,
    /// Only tracks that were never played
    #[bpaf(long)]
    pub unplayed: bool,
}

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Query(
    /// Free-text query over title, artist and album
    #[bpaf(positional("QUERY"), optional)]
    pub Option<String>,
);

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct FactType(
    /// A fact type, such as `genre` or `label`
    #[bpaf(positional("TYPE"))]
    pub Name,
);

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Added(
    /// Only tracks added in the last DAYS days, e.g. -7
    #[bpaf(long("added"), argument("DAYS"), optional)]
    pub Option<DaysBack>,
);

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Yes(
    /// Do not ask for confirmation
    #[bpaf(short('y'), long("yes"), switch)]
    pub bool,
);

#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_sources)]
pub struct SourceName(#[bpaf(positional("SOURCE"))] pub Name);

/// Runs on every TAB: never fails, and filters by what was typed.
fn complete_sources(typed: &SourceName) -> Vec<Candidate> {
    ["bandcamp", "beatport", "upload"]
        .into_iter()
        .filter(|s| s.starts_with(&typed.0.to_string()))
        .map(|s| (s.to_string(), None))
        .collect()
}

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Apply(
    /// Re-download anything that is out of date
    #[bpaf(long("apply"), switch)]
    pub bool,
);

#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = files)]
pub struct AudioFile(
    /// An audio file to upload
    #[bpaf(positional("FILE"))]
    pub PathBuf,
);

// A choice value: an ordinary enum with `FromStr`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortKey {
    Bpm,
    Title,
    Artist,
}

impl FromStr for SortKey {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "bpm" => Ok(Self::Bpm),
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            other => Err(format!("`{other}` is not one of: bpm, title, artist")),
        }
    }
}

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct SortBy(
    /// What to sort by: bpm, title or artist
    #[bpaf(positional("KEY"))]
    pub SortKey,
);

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Descending(
    /// Sort descending (Z→A, high→low)
    #[bpaf(short('d'), long("descending"), switch)]
    pub bool,
);

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct ShellArg(
    /// A shell to print completions for: bash, zsh or fish
    #[bpaf(positional("SHELL"))]
    pub hicli::Shell,
);

// ---- commands -------------------------------------------------------------

/// Check that the node answers
#[command]
fn ping(ctx: &Ctx) {
    println!("pong from {}", ctx.node);
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
            ctx.node, q.0, f.artist, f.bpm, f.unplayed, added.0
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
fn sort(desc: Descending, by: SortBy) {
    println!("sorting by {:?}, descending: {}", by.0, desc.0);
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
            assert_eq!(globals.node.host, "pi.local");
        },
        set_output_needs_a_name: { playback "set-output" } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("NAME"));
        },
        output_names_are_checked: { playback "set-output" HDMI } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
        },
        added_takes_the_glued_form: { search "--added=-7" } => |r| {
            assert!(r.is_ok(), "--added=-7 parses");
        },
        added_rejects_a_separate_negative_word: { search "--added" "-7" } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("try `--added=-7`"));
        },
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

    /// The usage paragraph of `args`' help page as `playr` prints it: from
    /// `Usage:` to the next blank line, wrapping kept.
    fn usage(args: &[&str]) -> String {
        let page = app()
            .bin_name("playr")
            .parse(args.iter().copied())
            .err()
            .expect("--help short-circuits")
            .unwrap_stdout();
        let at = page.find("Usage:").expect("a usage line");
        page[at..]
            .split("\n\n")
            .next()
            .unwrap_or_default()
            .to_string()
    }

    // The README pastes these usage lines; this keeps them true.
    #[test]
    fn usage_lines_match_the_readme() {
        assert_eq!(
            usage(&["search", "--help"]),
            "Usage: playr search (COMMAND ... | [--added=DAYS] [--artist=NAME] [--bpm=BPM] [--unplayed] [QUERY])"
        );
        assert_eq!(
            usage(&["source", "--help"]),
            "Usage: playr source (COMMAND ... | SOURCE COMMAND ...)"
        );
        assert_eq!(usage(&["sort", "--help"]), "Usage: playr sort [-d] KEY");
    }

    #[test]
    fn source_names_complete() {
        assert_eq!(app().complete(cli! { source b }), ["bandcamp", "beatport"]);
    }

    #[test]
    fn the_command_is_still_a_function() {
        let ctx = Ctx {
            node: Node::localhost(),
        };
        assert!(upload(AudioFile("/no/such/file.flac".into()), &ctx).is_err());
    }
}
