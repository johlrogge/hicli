//! A spelled type parses through its own `FromStr` first, and every spelled
//! parse error names the argument (ADR-018). Everything is in-process.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;

use hicli::bpaf::{pure, ParseFailure, Parser};
use hicli::{app, cli, command, exit_code_for, App, Arg, Shell};

/// The shared context: nothing here runs a command.
pub struct Ctx;

/// How many to take
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arg)]
#[arg(long = "limit", short = 'l', metavar = "N")]
pub struct Limit(pub NonZeroUsize);

impl FromStr for Limit {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Limit).map_err(|_| "must be at least 1")
    }
}

/// At most this many
#[derive(Debug, Clone, Arg)]
#[arg(long = "max", metavar = "N")]
pub struct Max(pub Option<Limit>);

/// An existing file
#[derive(Debug, Clone, Arg)]
#[arg(positional = "PATH", complete = files)]
pub struct Existing(pub PathBuf);

impl FromStr for Existing {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = PathBuf::from(s);
        if path.is_file() {
            Ok(Existing(path))
        } else {
            Err("does not exist")
        }
    }
}

/// An audio file or a ZIP archive
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(positional = "FILE", complete = files)]
pub struct Upload {
    /// Where the file is.
    pub path: PathBuf,
    /// Whether it is a ZIP archive.
    pub zip: bool,
}

impl FromStr for Upload {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let zip = if s.ends_with(".zip") {
            true
        } else if s.ends_with(".flac") {
            false
        } else {
            return Err("not audio or a ZIP archive");
        };
        Ok(Upload {
            path: PathBuf::from(s),
            zip,
        })
    }
}

/// A shell
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SHELL")]
pub struct ShellArg(pub Shell);

/// A POSIX shell
#[derive(Debug, Clone, Arg)]
#[arg(long = "shell", metavar = "SHELL")]
pub struct PosixShell(pub Shell);

impl FromStr for PosixShell {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<Shell>().map_err(|e| e.to_string())? {
            Shell::Fish => Err("not a POSIX shell".to_string()),
            shell => Ok(PosixShell(shell)),
        }
    }
}

/// take some
#[command]
pub fn take(limit: Limit, ctx: &Ctx) {
    let _ = (limit, ctx);
}

/// take at most some
#[command]
pub fn top(max: Max, ctx: &Ctx) {
    let _ = (max, ctx);
}

/// read a file
#[command]
pub fn read(path: Existing, ctx: &Ctx) {
    let _ = (path, ctx);
}

/// send a file
#[command]
pub fn send(file: Upload, ctx: &Ctx) {
    let _ = (file, ctx);
}

/// pick a shell
#[command]
pub fn shell(shell: ShellArg, ctx: &Ctx) {
    let _ = (shell, ctx);
}

/// pick a POSIX shell
#[command]
pub fn posix(shell: PosixShell, ctx: &Ctx) {
    let _ = (shell, ctx);
}

/// pick a shell, with a help of its own
///
/// # Arguments
///
/// * `shell` - Which shell
#[command]
pub fn which(shell: ShellArg, ctx: &Ctx) {
    let _ = (shell, ctx);
}

fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [take, top, read, send, shell, posix, which],
    }
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

/// What a revision-`rev` completion request for `words` comes back as.
fn complete_at(rev: usize, words: &[&str]) -> String {
    let args: Vec<String> = std::iter::once(format!("--bpaf-complete-rev={rev}"))
        .chain(words.iter().map(|w| (*w).to_string()))
        .collect();
    match app().parse(args) {
        Err(ParseFailure::Completion(out)) => out,
        Err(ParseFailure::Stderr(msg)) => panic!("rev {rev}: usage error:\n{msg}"),
        Err(ParseFailure::Stdout(..)) => panic!("rev {rev}: rendered as help"),
        Ok(_) => panic!("rev {rev}: parsed instead of completing"),
    }
}

#[test]
fn the_types_own_from_str_wins_over_its_fields() {
    for args in [cli! { take --limit 0 }, cli! { take -l 0 }] {
        assert_eq!(
            error_of(args),
            "couldn't parse `0`: `--limit`: must be at least 1"
        );
    }
    let l: Limit = parse(&["-l", "3"]);
    assert_eq!(l.0.get(), 3);
}

#[test]
fn each_word_of_an_option_parses_through_the_inner_type() {
    assert_eq!(
        error_of(cli! { top --max 0 }),
        "couldn't parse `0`: `--max`: must be at least 1"
    );
    let m: Max = parse(&[]);
    assert_eq!(m.0, None);
    let m: Max = parse(&["--max", "2"]);
    assert_eq!(m.0.map(|l| l.0.get()), Some(2));
}

#[test]
fn a_path_type_checks_its_own_rule() {
    let e: Existing = parse(&["Cargo.toml"]);
    assert_eq!(e.0, PathBuf::from("Cargo.toml"));
    assert_eq!(
        error_of(cli! { read no/such/file }),
        "couldn't parse `no/such/file`: `PATH`: does not exist"
    );
}

#[test]
fn shell_completion_survives_a_failing_parse() {
    for typed in ["", "Car"] {
        let out = complete_at(8, &["read", typed]);
        assert!(out.contains("_filedir"), "{typed:?}: got:\n{out}");
    }
}

#[test]
fn a_struct_with_several_fields_parses_whole() {
    let u: Upload = parse(&["a.zip"]);
    assert_eq!(
        u,
        Upload {
            path: PathBuf::from("a.zip"),
            zip: true
        }
    );
    let u: Upload = parse(&["a.flac"]);
    assert!(!u.zip);
    assert_eq!(
        error_of(cli! { send a.txt }),
        "couldn't parse `a.txt`: `FILE`: not audio or a ZIP archive"
    );
}

#[test]
fn a_newtype_over_a_choice_lists_and_completes_its_names() {
    assert_eq!(
        help_of(cli! { shell --help }),
        "pick a shell\n\
         \n\
         Usage: shell SHELL\n\
         \n\
         Available positional items:\n\
         \x20   SHELL       A shell [bash, zsh, fish]\n\
         \n\
         Available options:\n\
         \x20   -h, --help  Prints help information\n"
    );
    assert_eq!(app().complete(cli! { shell "" }), ["bash", "zsh", "fish"]);
}

#[test]
fn a_choice_newtype_with_its_own_rule_still_lists_and_completes() {
    assert_eq!(
        error_of(cli! { posix --shell fish }),
        "couldn't parse `fish`: `--shell`: not a POSIX shell"
    );
    let p: PosixShell = parse(&["--shell", "zsh"]);
    assert_eq!(p.0, Shell::Zsh);
    assert_eq!(
        app().complete(cli! { posix --shell "" }),
        ["bash", "zsh", "fish"]
    );
    assert_eq!(
        help_of(cli! { posix --help }),
        "pick a POSIX shell\n\
         \n\
         Usage: posix --shell=SHELL\n\
         \n\
         Available options:\n\
         \x20       --shell=SHELL  A POSIX shell [bash, zsh, fish]\n\
         \x20   -h, --help         Prints help information\n"
    );
}

#[test]
fn an_override_row_lists_the_names_too() {
    assert_eq!(
        help_of(cli! { which --help }),
        "pick a shell, with a help of its own\n\
         \n\
         Usage: which SHELL\n\
         \n\
         Available positional items:\n\
         \x20   SHELL       Which shell [bash, zsh, fish]\n\
         \n\
         Available options:\n\
         \x20   -h, --help  Prints help information\n"
    );
}

#[cfg(unix)]
#[test]
fn a_non_utf8_word_for_a_path_type_with_a_rule_is_named() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let raw = OsStr::from_bytes(b"caf\xe9").to_os_string();
    let e = <Existing as Arg>::parser()
        .to_options()
        .run_inner(&[raw][..])
        .expect_err("not UTF-8")
        .unwrap_stderr();
    assert!(e.ends_with("`PATH`: not valid UTF-8"), "got: {e}");
}
