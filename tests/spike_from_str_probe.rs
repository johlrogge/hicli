//! TEMPORARY (ADR-018 Step 1 proof gate; Step 4 deletes it): the hand-written
//! shape the spelled derive will generate, compiled and run for real.

// The borrow is the probe: without it `FieldFromStr` / `FieldFromOs` are never
// reached. Clippy only sees the case where `SelfFromStr` applies.
#![allow(clippy::needless_borrow)]

use std::path::PathBuf;
use std::str::FromStr;

use hicli::__private::{named, FromStrOf};
#[allow(unused_imports)]
use hicli::__private::{FieldFromOs as _, FieldFromStr as _, SelfFromOs as _, SelfFromStr as _};
use hicli::bpaf::{positional, pure, ParseFailure, Parser};
use hicli::{app, command, complete_files, App, Arg, ArgHelp};

/// The shared context: nothing here runs a command.
pub struct Ctx;

/// The rule both path types share.
fn existing(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    if p.is_file() {
        Ok(p)
    } else {
        Err("does not exist".to_string())
    }
}

/// An existing file, read as a `String` head (Step 3 chain (a)).
#[derive(Debug)]
pub struct Existing(pub PathBuf);

impl FromStr for Existing {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        existing(s).map(Existing)
    }
}

impl ArgHelp for Existing {
    fn parser_with_help(help: &'static str) -> impl Parser<Self> {
        positional::<String>("PATH").help(help).parse(|w: String| {
            (&FromStrOf::<Existing, PathBuf>(Existing))
                .parse_word(w.as_str())
                .map_err(|e| named("PATH", e))
        })
    }
}

impl Arg for Existing {
    fn parser() -> impl Parser<Self> {
        <Existing as ArgHelp>::parser_with_help("An existing file")
    }
}

/// An existing file, completed by the shell: the completion wraps the typed
/// head, and the parse follows (order 1).
#[derive(Debug)]
pub struct Outer(pub PathBuf);

impl FromStr for Outer {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        existing(s).map(Outer)
    }
}

impl ArgHelp for Outer {
    fn parser_with_help(help: &'static str) -> impl Parser<Self> {
        complete_files(positional::<PathBuf>("PATH").help(help)).parse(|w: PathBuf| {
            (&FromStrOf::<Outer, PathBuf>(Outer))
                .parse_os(w)
                .map_err(|e| named("PATH", e))
        })
    }
}

impl Arg for Outer {
    fn parser() -> impl Parser<Self> {
        <Outer as ArgHelp>::parser_with_help("An existing file")
    }
}

/// The same, with the completion wrapped around the parse (order 2).
#[derive(Debug)]
pub struct Inner(pub PathBuf);

impl FromStr for Inner {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        existing(s).map(Inner)
    }
}

impl ArgHelp for Inner {
    fn parser_with_help(help: &'static str) -> impl Parser<Self> {
        complete_files(
            positional::<PathBuf>("PATH")
                .help(help)
                .parse(|w: PathBuf| {
                    (&FromStrOf::<Inner, PathBuf>(Inner))
                        .parse_os(w)
                        .map_err(|e| named("PATH", e))
                }),
        )
    }
}

impl Arg for Inner {
    fn parser() -> impl Parser<Self> {
        <Inner as ArgHelp>::parser_with_help("An existing file")
    }
}

/// read a file (order 1)
#[command]
fn read(p: Outer) {
    let _ = p.0;
}

/// read a file (order 2)
#[command]
fn read2(p: Inner) {
    let _ = p.0;
}

/// The tree.
fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [read, read2],
    }
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
fn c_the_generated_shape_compiles_and_runs() {
    let ok = <Existing as Arg>::parser()
        .to_options()
        .run_inner(&["Cargo.toml"][..])
        .expect("an existing file");
    assert_eq!(ok.0, PathBuf::from("Cargo.toml"));
    let e = <Existing as Arg>::parser()
        .to_options()
        .run_inner(&["no/such"][..])
        .expect_err("no such file")
        .unwrap_stderr();
    assert_eq!(e, "couldn't parse `no/such`: `PATH`: does not exist");
}

#[test]
fn e_order_1_completion_wraps_the_head_and_survives_a_failing_parse() {
    for typed in ["", "Car"] {
        let out = complete_at(8, &["read", typed]);
        assert!(out.contains("_filedir"), "{typed:?}: got:\n{out}");
    }
}

#[test]
fn e_order_2_completion_wraps_the_parse_and_survives_a_failing_parse() {
    for typed in ["", "Car"] {
        let out = complete_at(8, &["read2", typed]);
        assert!(out.contains("_filedir"), "{typed:?}: got:\n{out}");
    }
}

#[cfg(unix)]
#[test]
fn d_a_non_utf8_word_through_the_command_is_named() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let raw = OsStr::from_bytes(b"caf\xe9").to_os_string();
    let e = <Outer as Arg>::parser()
        .to_options()
        .run_inner(&[raw][..])
        .expect_err("not UTF-8")
        .unwrap_stderr();
    assert!(e.ends_with("`PATH`: not valid UTF-8"), "got: {e}");
}
