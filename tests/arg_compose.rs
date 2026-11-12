//! Composed `#[derive(Arg)]` end to end (ADR-019): a struct with named fields,
//! no spelling key and no `#[bpaf(…)]` composes its fields' argument types in
//! declaration order. Each field keeps its type's help, completion and named
//! errors; a field's doc comment overrides its type's row through `ArgHelp`;
//! the struct's doc comment is a section heading.

// A field's doc comment *is* an override, so a field that keeps its type's
// help has none — and a struct's doc is a heading, so one without a heading
// has none either. `missing_docs` cannot tell those apart from forgetting.
#![allow(missing_docs)]

use hicli::bpaf::{Bpaf, Parser};
use hicli::{app, cli, command, exit_code_for, group, App, Arg, Candidate, Choice};

/// The shared context: nothing here runs a command.
pub struct Ctx;

/// A target format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Format {
    /// Keep the source format
    Original,
    /// Apple's lossless container
    Aiff,
}

/// Only tracks by this artist
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "artist", metavar = "NAME")]
pub struct Artist(pub Option<String>);

/// Target format for lossless sources
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "lossless-format", metavar = "FORMAT")]
pub struct LosslessFormat(pub Option<Format>);

/// Output directory
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "output", metavar = "DIR", complete = dirs)]
pub struct OutputDir(pub Option<std::path::PathBuf>);

/// Only tracks that were never played
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long)]
pub struct Unplayed(pub bool);

/// A playlist to export
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "playlist", metavar = "NAME", complete = playlists)]
pub struct Playlist(pub Vec<String>);

/// The word typed so far, completed against two playlists.
fn playlists(typed: &str) -> Vec<Candidate> {
    ["techno", "tango"]
        .into_iter()
        .filter(|p| p.starts_with(typed))
        .map(|p| (p.to_string(), None))
        .collect()
}

/// A port on the node
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "port", metavar = "PORT")]
pub struct Port(pub Option<u16>);

/// Free-text query
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(positional = "QUERY")]
pub struct Query(pub Option<String>);

/// Export options
#[derive(Debug, Clone, PartialEq, Arg)]
pub struct ExportArgs {
    pub artist: Artist,
    /// Convert lossless sources to this
    pub format: LosslessFormat,
    /// Where the export goes. Default: ./export/
    pub output: OutputDir,
    pub unplayed: Unplayed,
    pub playlist: Playlist,
    pub query: Query,
}

// An undocumented composite: no heading.
#[derive(Debug, Clone, PartialEq, Arg)]
pub struct RekordboxArgs {
    /// Defaults to ~/Music/rekordbox
    pub output: OutputDir,
    pub port: Port,
}

/// Export tracks
#[command]
pub fn export(a: ExportArgs, ctx: &Ctx) {
    let _ = (a, ctx);
}

/// Export to rekordbox
#[command]
pub fn rekordbox(a: RekordboxArgs, ctx: &Ctx) {
    let _ = (a, ctx);
}

// A bpaf-mode struct that embeds a composed one through its constructor.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Embedding {
    #[bpaf(external(rekordbox_args))]
    pub inner: RekordboxArgs,
    #[bpaf(long)]
    pub dry_run: bool,
}

/// Embed one in another
#[command]
pub fn embed(e: Embedding, ctx: &Ctx) {
    let _ = (e, ctx);
}

/// A source's name
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(positional = "SOURCE")]
pub struct SourceName(pub String);

// The value a curried group parses: a composite.
#[derive(Debug, Clone, PartialEq, Arg)]
pub struct SourceRef {
    pub port: Port,
    pub name: SourceName,
}

/// Music sources
#[group(args = SourceRef)]
pub mod source {
    use super::*;

    /// Sync the source
    #[command(curried)]
    pub fn sync(s: SourceRef, ctx: &Ctx) {
        let _ = (s, ctx);
    }
}

fn app() -> App<(), Ctx> {
    app! { globals: hicli::bpaf::pure(()), commands: [export, rekordbox, embed, source] }
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

/// The composite `app <args…>` parsed `export` into.
fn parsed_export(args: Vec<String>) -> ExportArgs {
    let p = <ExportArgs as Arg>::parser().to_options();
    let words: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    // The command parses too: the composite is its only argument.
    app().parse(args.clone()).expect("export parses");
    p.run_inner(&words[..]).expect("the composite parses")
}

#[test]
fn the_help_is_the_fields_rows_under_the_structs_heading() {
    assert_eq!(
        help_of(cli! { export --help }),
        "Export tracks\n\
         \n\
         Usage: export [--artist=NAME] [--lossless-format=FORMAT] [--output=DIR] [--unplayed]\n\
         \x20             [--playlist=NAME]... [QUERY]\n\
         \n\
         Export options\n\
         \x20       --artist=NAME    Only tracks by this artist\n\
         \x20       --lossless-format=FORMAT  Convert lossless sources to this [original, aiff]\n\
         \x20       --output=DIR     Where the export goes. Default: ./export/\n\
         \x20       --unplayed       Only tracks that were never played\n\
         \x20       --playlist=NAME  A playlist to export\n\
         \x20   QUERY                Free-text query\n\
         \n\
         Available options:\n\
         \x20   -h, --help           Prints help information\n"
    );
}

#[test]
fn an_undocumented_composite_has_no_heading_and_one_type_reads_differently() {
    assert_eq!(
        help_of(cli! { rekordbox --help }),
        "Export to rekordbox\n\
         \n\
         Usage: rekordbox [--output=DIR] [--port=PORT]\n\
         \n\
         Available options:\n\
         \x20       --output=DIR  Defaults to ~/Music/rekordbox\n\
         \x20       --port=PORT   A port on the node\n\
         \x20   -h, --help        Prints help information\n"
    );
}

#[test]
fn fields_are_present_and_absent() {
    let a = parsed_export(cli! { export });
    assert_eq!(
        a,
        ExportArgs {
            artist: Artist(None),
            format: LosslessFormat(None),
            output: OutputDir(None),
            unplayed: Unplayed(false),
            playlist: Playlist(vec![]),
            query: Query(None),
        }
    );
    let a = parsed_export(cli! {
        export --artist CBL --lossless-format aiff --output ./out --unplayed
            --playlist techno --playlist tango jazz
    });
    assert_eq!(
        a,
        ExportArgs {
            artist: Artist(Some("CBL".into())),
            format: LosslessFormat(Some(Format::Aiff)),
            output: OutputDir(Some("./out".into())),
            unplayed: Unplayed(true),
            playlist: Playlist(vec!["techno".into(), "tango".into()]),
            query: Query(Some("jazz".into())),
        }
    );
}

#[test]
fn each_fields_error_is_named_by_its_types_spelling() {
    assert_eq!(
        error_of(cli! { export --lossless-format aif }),
        "couldn't parse `aif`: `--lossless-format`: expected `original` or `aiff`; did you mean `aiff`?"
    );
    assert_eq!(
        error_of(cli! { rekordbox --port http }),
        "couldn't parse `http`: `--port`: invalid digit found in string"
    );
}

#[test]
fn each_field_completes_through_its_type() {
    assert_eq!(
        app().complete(cli! { export --lossless-format "" }),
        ["original", "aiff"]
    );
    assert_eq!(
        app().complete(cli! { export --playlist "" }),
        ["techno", "tango"]
    );
    assert_eq!(app().complete(cli! { export --playlist te }), ["techno"]);
}

#[test]
fn a_bpaf_mode_struct_embeds_a_composed_one() {
    let p = <Embedding as Arg>::parser().to_options();
    let e = p
        .run_inner(&["--port", "7700", "--dry-run"][..])
        .expect("parses");
    assert_eq!(e.inner.port, Port(Some(7700)));
    assert!(e.dry_run);
    app()
        .parse(cli! { embed --dry-run })
        .expect("the command parses");
}

#[test]
fn a_curried_group_parses_a_composite() {
    let (_, inv) = app()
        .parse(cli! { source --port 1 bandcamp sync })
        .expect("parses");
    assert_eq!(inv.path(), ["source", "sync"]);
}

// --- globals -------------------------------------------------------------------

/// The node to talk to
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "node", metavar = "HOST")]
pub struct NodeOpt(pub Option<String>);

/// Print more
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "verbose", short = 'v')]
pub struct Verbose(pub bool);

/// Global options
#[derive(Debug, Clone, PartialEq, Arg)]
pub struct Globals {
    pub node: NodeOpt,
    pub verbose: Verbose,
}

/// Check the node
#[command]
pub fn ping(ctx: &Ctx) {
    let _ = ctx;
}

fn with_globals() -> App<Globals, Ctx> {
    app! { globals: globals(), commands: [ping] }
}

#[test]
fn a_composed_globals_is_a_parser() {
    let (g, _) = with_globals()
        .parse(cli! { --node pi.local ping -v })
        .expect("parses");
    assert_eq!(
        g,
        Globals {
            node: NodeOpt(Some("pi.local".into())),
            verbose: Verbose(true)
        }
    );
    let (g, _) = with_globals()
        .parse(cli! { ping --node pi.local })
        .expect("a global after the command");
    assert_eq!(g.node, NodeOpt(Some("pi.local".into())));
    let help = with_globals()
        .parse(cli! { --help })
        .err()
        .expect("help")
        .unwrap_stdout();
    assert_eq!(
        help,
        "Usage: [--node=HOST] [-v] COMMAND ...\n\
         \n\
         Global options\n\
         \x20       --node=HOST  The node to talk to\n\
         \x20   -v, --verbose    Print more\n\
         \n\
         Available options:\n\
         \x20   -h, --help       Prints help information\n\
         \n\
         Available commands:\n\
         \x20   ping             Check the node\n"
    );
}

// --- `Option<T>` fields ----------------------------------------------------------

/// A tempo, 20 to 300
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arg)]
#[arg(long = "bpm", metavar = "BPM")]
pub struct BpmArg(pub u16);

impl std::str::FromStr for BpmArg {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(b @ 20..=300) => Ok(BpmArg(b)),
            _ => Err("a tempo is from 20 to 300"),
        }
    }
}

/// The node's host
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(long = "host", metavar = "HOST")]
pub struct Host(pub String);

/// Options, every one of which may be left out
#[derive(Debug, Clone, PartialEq, Arg)]
pub struct Optionals {
    pub bpm: Option<BpmArg>,
    /// The node to sync with
    pub host: Option<Host>,
}

fn optionals_app() -> App<Optionals, Ctx> {
    app! { globals: optionals(), commands: [ping] }
}

#[test]
fn an_option_field_is_its_types_parser_made_optional() {
    let (g, _) = optionals_app().parse(cli! { ping }).expect("parses");
    assert_eq!(
        g,
        Optionals {
            bpm: None,
            host: None
        }
    );
    let (g, _) = optionals_app()
        .parse(cli! { --bpm 128 --host pi.local ping })
        .expect("parses");
    assert_eq!(
        g,
        Optionals {
            bpm: Some(BpmArg(128)),
            host: Some(Host("pi.local".into()))
        }
    );
    let e = optionals_app()
        .parse(cli! { --bpm 9 ping })
        .err()
        .expect("a usage error");
    assert_eq!(
        e.unwrap_stderr(),
        "couldn't parse `9`: `--bpm`: a tempo is from 20 to 300"
    );
    let help = optionals_app()
        .parse(cli! { --help })
        .err()
        .expect("help")
        .unwrap_stdout();
    assert_eq!(
        help,
        "Usage: [--bpm=BPM] [--host=HOST] COMMAND ...\n\
         \n\
         Options, every one of which may be left out\n\
         \x20       --bpm=BPM    A tempo, 20 to 300\n\
         \x20       --host=HOST  The node to sync with\n\
         \n\
         Available options:\n\
         \x20   -h, --help       Prints help information\n\
         \n\
         Available commands:\n\
         \x20   ping             Check the node\n"
    );
}

/// The node's address
#[derive(Debug, Clone, PartialEq, Arg)]
#[arg(long = "addr", metavar = "ADDR", env = "HICLI_TEST_NODE")]
pub struct OneGlobal(pub Option<std::net::SocketAddr>);

fn with_one_global() -> App<OneGlobal, Ctx> {
    app! { globals: one_global(), commands: [ping] }
}

#[test]
fn a_single_global_is_a_spelled_newtype_with_an_env_fallback() {
    let (g, _) = with_one_global()
        .parse(cli! { --addr 127.0.0.1:7700 ping })
        .expect("parses");
    assert_eq!(g.0, Some("127.0.0.1:7700".parse().expect("an address")));
    let (g, _) = with_one_global()
        .parse(cli! { ping --addr 127.0.0.1:1 })
        .expect("a global after the command");
    assert!(g.0.is_some());
    let e = with_one_global()
        .parse(cli! { --addr pi.local ping })
        .err()
        .expect("a usage error");
    assert_eq!(exit_code_for(&e), 2);
    assert_eq!(
        e.unwrap_stderr(),
        "couldn't parse `pi.local`: `--addr`: invalid socket address syntax"
    );
    let help = with_one_global()
        .parse(cli! { --help })
        .err()
        .expect("help")
        .unwrap_stdout();
    // Read, not changed: other tests in this binary run alongside.
    let env = match std::env::var_os("HICLI_TEST_NODE") {
        None => ": N/A".to_string(),
        Some(v) => format!(" = {:?}", v.to_string_lossy()),
    };
    assert_eq!(
        help,
        format!(
            "Usage: [--addr=ADDR] COMMAND ...\n\
         \n\
         Available options:\n\
         \x20       --addr=ADDR  The node's address\n\
         \x20                    [env:HICLI_TEST_NODE{env}]\n\
         \x20   -h, --help       Prints help information\n\
         \n\
         Available commands:\n\
         \x20   ping             Check the node\n"
        )
    );
}
