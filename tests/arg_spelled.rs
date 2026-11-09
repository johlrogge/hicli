//! Spelled `#[derive(Arg)]` end to end (issue #8, step 2): a type spelled by
//! `#[arg(positional|long|short …)]` builds its own parser, its help row is its
//! doc comment's first paragraph, and a `Choice` inside it completes on its own.
//!
//! None of the spelled types below derive `Bpaf`. Everything is in-process.

use hicli::bpaf::{pure, Bpaf, Parser};
use hicli::{app, cli, command, exit_code_for, App, Arg, Candidate, Choice};

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

/// Sort the listing
/// by this field
///
/// Developer prose after the first paragraph: rustdoc only, never printed.
#[derive(Debug, Clone, Arg)]
#[arg(long = "sort", short = 's', metavar = "FIELD")]
pub struct SortOpt(pub Option<SortField>);

/// Sort descending
#[derive(Debug, Clone, Arg)]
#[arg(long = "descending", short = 'd')]
pub struct Descending(pub bool);

/// What to sort by
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice, Arg)]
#[arg(positional = "KEY")]
pub enum SortKey {
    /// Beats per minute
    Bpm,
    /// Artist name
    Artist,
}

/// The ports a node listens on, filtered by the word typed so far.
fn known_ports(typed: &str) -> Vec<Candidate> {
    ["80", "443", "8080"]
        .iter()
        .filter(|p| p.starts_with(typed))
        .map(|p| ((*p).to_string(), None))
        .collect()
}

/// The node to talk to
#[derive(Debug, Clone, Arg)]
#[arg(long, metavar = "PORT", complete = known_ports)]
pub struct Port {
    /// The port number.
    pub port: u16,
}

/// Tags to apply
#[derive(Debug, Clone, Arg)]
#[arg(positional = "TAG")]
pub struct Tags(pub Vec<String>);

/// A file to read
#[derive(Debug, Clone, Arg)]
#[arg(positional = "FILE", complete = files)]
pub struct Input(pub std::path::PathBuf);

/// Where to write
#[derive(Debug, Clone, Arg)]
#[arg(positional)]
pub struct OutDir(pub std::path::PathBuf);

/// list tracks
#[command]
pub fn list(sort: SortOpt, desc: Descending, port: Port, ctx: &Ctx) {
    let _ = (sort, desc, port, ctx);
}

/// order tracks
#[command]
pub fn order(key: SortKey, ctx: &Ctx) {
    let _ = (key, ctx);
}

/// tag tracks
#[command]
pub fn tag(tags: Tags, ctx: &Ctx) {
    let _ = (tags, ctx);
}

/// read a file
#[command]
pub fn read(input: Input, out: OutDir, ctx: &Ctx) {
    let _ = (input, out, ctx);
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [list, order, tag, read] }
}

/// `app <args…>`'s help, which must exit 0.
fn help_of(args: Vec<String>) -> String {
    let e = app().parse(args).err().expect("--help short-circuits");
    assert_eq!(exit_code_for(&e), 0);
    e.unwrap_stdout()
}

/// `app <args…>`'s usage error, which must exit 2.
fn error_of(args: Vec<String>) -> String {
    let e = app().parse(args.clone()).err().expect("a usage error");
    assert_eq!(exit_code_for(&e), 2);
    assert_eq!(app().run_with_args(args, |_| Ok(Ctx)), 2);
    e.unwrap_stderr()
}

/// `T`'s parser run alone over `args`.
fn parse<T: Arg>(args: &[&str]) -> T {
    T::parser().to_options().run_inner(args).expect("parses")
}

#[test]
fn the_help_rows_are_the_types_first_paragraphs() {
    assert_eq!(
        help_of(cli! { list --help }),
        "list tracks\n\
         \n\
         Usage: list [-s=FIELD] [-d] --port=PORT\n\
         \n\
         Available options:\n\
         \x20   -s, --sort=FIELD  Sort the listing by this field [bpm, title]\n\
         \x20   -d, --descending  Sort descending\n\
         \x20       --port=PORT   The node to talk to\n\
         \x20   -h, --help        Prints help information\n"
    );
    assert_eq!(
        help_of(cli! { order --help }),
        "order tracks\n\
         \n\
         Usage: order KEY\n\
         \n\
         Available positional items:\n\
         \x20   KEY         What to sort by [bpm, artist]\n\
         \n\
         Available options:\n\
         \x20   -h, --help  Prints help information\n"
    );
    assert_eq!(
        help_of(cli! { read --help }),
        "read a file\n\
         \n\
         Usage: read FILE OUT-DIR\n\
         \n\
         Available positional items:\n\
         \x20   FILE        A file to read\n\
         \x20   OUT-DIR     Where to write\n\
         \n\
         Available options:\n\
         \x20   -h, --help  Prints help information\n"
    );
}

#[test]
fn spelled_types_parse() {
    let s: SortOpt = parse(&["--sort", "title"]);
    assert_eq!(s.0, Some(SortField::Title));
    let s: SortOpt = parse(&["-s", "bpm"]);
    assert_eq!(s.0, Some(SortField::Bpm));
    let s: SortOpt = parse(&[]);
    assert_eq!(s.0, None, "an absent Option is None");
    let d: Descending = parse(&["-d"]);
    assert!(d.0);
    let d: Descending = parse(&[]);
    assert!(!d.0);
    let k: SortKey = parse(&["artist"]);
    assert_eq!(k, SortKey::Artist);
    let p: Port = parse(&["--port", "8080"]);
    assert_eq!(p.port, 8080);
    let t: Tags = parse(&["a", "b"]);
    assert_eq!(t.0, ["a", "b"]);
    let t: Tags = parse(&[]);
    assert!(t.0.is_empty());
    let i: Input = parse(&["Cargo.toml"]);
    assert_eq!(i.0, std::path::Path::new("Cargo.toml"));

    let (_, inv) = app()
        .parse(cli! { list -s title -d --port 80 })
        .expect("parses");
    assert_eq!(inv.path(), ["list"]);
}

#[test]
fn spelled_errors_name_the_choices() {
    assert_eq!(
        error_of(cli! { list --sort bmp --port 80 }),
        "couldn't parse `bmp`: `--sort`: expected `bpm` or `title`; did you mean `bpm`?"
    );
    assert_eq!(
        error_of(cli! { order genre }),
        "couldn't parse `genre`: `KEY`: expected `bpm` or `artist`"
    );
    assert_eq!(
        error_of(cli! { list --port http }),
        "couldn't parse `http`: `--port`: invalid digit found in string"
    );
    assert_eq!(
        error_of(cli! { list }),
        "expected `--port=PORT`, pass `--help` for usage information"
    );
}

#[test]
fn spelled_types_complete() {
    // A Choice inside an option completes on its own, through the probe.
    assert_eq!(app().complete(cli! { list --sort "" }), ["bpm", "title"]);
    assert_eq!(app().complete(cli! { list --sort t }), ["title"]);
    assert_eq!(app().complete(cli! { list -s b }), ["bpm"]);
    // A self-spelled Choice enum completes its own names.
    assert_eq!(app().complete(cli! { order "" }), ["bpm", "artist"]);
    assert_eq!(app().complete(cli! { order a }), ["artist"]);
    // `complete = f` is handed the raw word — the empty one included, which
    // a `u16` would never parse.
    assert_eq!(
        app().complete(cli! { list --port "" }),
        ["80", "443", "8080"]
    );
    assert_eq!(app().complete(cli! { list --port 8 }), ["80", "8080"]);
    // A String with no Choice inside offers nothing.
    assert!(app().complete(cli! { tag "" }).is_empty());
}

// A bpaf struct embedding a spelled type through the constructor the derive
// wrote. The field's own doc is dropped by bpaf; the row shows the type's. Not
// a doc comment: on a bpaf struct that would be a group heading.
#[allow(missing_docs)]
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Filters {
    /// this field doc is never rendered
    #[bpaf(external(sort_opt))]
    pub sort: SortOpt,
}

/// filter tracks
#[command]
pub fn filter(f: Filters, ctx: &Ctx) {
    let _ = (f, ctx);
}

#[test]
fn an_external_field_shows_the_types_help() {
    let a: App<(), Ctx> = app! { globals: pure(()), commands: [filter] };
    let out = a
        .parse(cli! { filter --help })
        .err()
        .expect("help")
        .unwrap_stdout();
    assert_eq!(
        out,
        "filter tracks\n\
         \n\
         Usage: filter [-s=FIELD]\n\
         \n\
         Available options:\n\
         \x20   -s, --sort=FIELD  Sort the listing by this field [bpm, title]\n\
         \x20   -h, --help        Prints help information\n"
    );
    let f: Filters = parse(&["-s", "bpm"]);
    assert_eq!(f.sort.0, Some(SortField::Bpm));
}

#[cfg(unix)]
#[test]
fn a_path_positional_takes_a_non_utf8_word() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let word = OsStr::from_bytes(b"caf\xe9");
    let out = <OutDir as Arg>::parser()
        .to_options()
        .run_inner(hicli::bpaf::Args::from(&[word][..]))
        .expect("a non-UTF-8 path parses");
    assert_eq!(out.0.as_os_str(), word);
}
