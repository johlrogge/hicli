//! Regression tests for johlrogge/hicli#5: an option whose *value* starts with
//! a hyphen. mdma-cli spells relative dates as `--added -7`, which clap only
//! accepts with `allow_hyphen_values`.
//!
//! Two argument types are exercised through ordinary `#[command]`s:
//!
//! - `Added`, a plain `#[bpaf(long, argument)]` — what a consumer writes first.
//! - `Since`, a hand-written `Arg` impl using the idiomatic bpaf 0.9 pattern for
//!   a value that may look like a flag: the flag and an `any` value glued
//!   together by `adjacent`, with the ordinary `argument` as the alternative
//!   for the `--since=-7` spelling.
//!
//! The finding, pinned by the tests below: bpaf splits `-7` into a short flag
//! *before* any parser runs, so `--added -7` is a usage error with a plain
//! `argument`, while `--added=-7` parses as-is. The `adjacent` + `any` pattern
//! accepts both, and with the separate-word branch `hide()`n, `--help` lists
//! the option once as `--since=DAYS`. #5 settled on documenting this recipe (in
//! `src/arg.rs`) rather than shipping a helper.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{any, construct, long, pure, Bpaf, Parser};
use hicli::{app, cli, command, exit_code_for, App, Arg, Failure};

/// Records what ran, so a test can assert the parsed value.
pub struct Ctx {
    log: Rc<RefCell<Vec<String>>>,
}

/// A relative date, written the obvious way: a plain long option.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Added {
    #[bpaf(long("added"), argument("DAYS"))]
    added: String,
}

/// A relative date that accepts a hyphen-leading value in either spelling.
///
/// `Arg` is written by hand — the escape hatch for anything `#[derive(Bpaf)]`
/// cannot spell — so no hicli API is involved.
#[derive(Debug, Clone)]
pub struct Since(String);

impl Arg for Since {
    fn parser() -> impl Parser<Self> {
        hyphen_value("since", "DAYS").map(Since)
    }
}

/// `--name VALUE` where `VALUE` may start with `-`, plus `--name=VALUE`.
///
/// The separate-word form is `long(name)` immediately followed by *any* item
/// (`any` takes the raw item, flag-shaped or not), and `adjacent` stops the two
/// being matched apart. The `=` form is left to an ordinary `argument`, which
/// already reads `--name=-7` because the value is glued to the flag. The
/// separate form is `hide()`n so `--help` lists the option once, as the
/// ordinary `argument` renders it. This is the recipe `src/arg.rs` documents
/// (#5 settled on documenting it rather than shipping a helper).
fn hyphen_value(name: &'static str, metavar: &'static str) -> impl Parser<String> {
    let flag = long(name).req_flag(());
    let value = any::<String, _, _>(metavar, Some);
    // Hidden, so `--help` shows only the `argument` branch below. (Bound, not
    // inlined: `construct!` accepts only plain idents.)
    let separate = construct!(flag, value).adjacent().map(|((), v)| v).hide();
    let joined = long(name).argument::<String>(metavar);
    construct!([separate, joined])
}

/// list by date added
#[command]
fn list(a: Added, ctx: &Ctx) {
    ctx.log.borrow_mut().push(format!("list:{}", a.added));
}

/// list by date, hyphen-tolerant
#[command]
fn recent(s: Since, ctx: &Ctx) {
    ctx.log.borrow_mut().push(format!("recent:{}", s.0));
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [list, recent] }
}

/// Run `args`, returning the exit code and what ran.
fn run(args: &[&str]) -> (u8, Vec<String>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let ctx_log = Rc::clone(&log);
    let code = app().run_with_args(args.iter().copied(), move |()| -> Result<Ctx, Failure> {
        Ok(Ctx { log: ctx_log })
    });
    let ran = log.borrow().clone();
    (code, ran)
}

// --- the plain `argument` ------------------------------------------------------

#[test]
fn a_plain_argument_rejects_a_separate_hyphen_value() {
    // bpaf tokenises `-7` as the short flag `-7` before parsing, and `argument`
    // only takes a following *word* — so this is a usage error, not a value.
    let e = app()
        .parse(cli! { list "--added" "-7" })
        .err()
        .expect("expected a parse failure");
    assert_eq!(exit_code_for(&e), 2);
    let msg = e.unwrap_stderr();
    assert!(msg.contains("DAYS"), "{msg}");
}

#[test]
fn a_plain_argument_accepts_an_equals_hyphen_value() {
    let (code, ran) = run(&["list", "--added=-7"]);
    assert_eq!(code, 0);
    assert_eq!(ran, ["list:-7"]);
}

#[test]
fn a_plain_argument_still_accepts_an_ordinary_value() {
    let (code, ran) = run(&["list", "--added", "7"]);
    assert_eq!(code, 0);
    assert_eq!(ran, ["list:7"]);
}

// --- the `adjacent` + `any` pattern ----------------------------------------------

#[test]
fn the_adjacent_pattern_accepts_a_separate_hyphen_value() {
    let (code, ran) = run(&["recent", "--since", "-7"]);
    assert_eq!(code, 0);
    assert_eq!(ran, ["recent:-7"]);
}

#[test]
fn the_adjacent_pattern_accepts_an_equals_hyphen_value() {
    let (code, ran) = run(&["recent", "--since=-7"]);
    assert_eq!(code, 0);
    assert_eq!(ran, ["recent:-7"]);
}

#[test]
fn the_adjacent_pattern_accepts_an_ordinary_value() {
    let (code, ran) = run(&["recent", "--since", "7"]);
    assert_eq!(code, 0);
    assert_eq!(ran, ["recent:7"]);
}

#[test]
fn the_adjacent_pattern_still_requires_a_value() {
    let e = app()
        .parse(cli! { recent "--since" })
        .err()
        .expect("expected a parse failure");
    assert_eq!(exit_code_for(&e), 2);
    // A usage error, on stderr — not help, not completion.
    let _ = e.unwrap_stderr();
}

#[test]
fn the_adjacent_pattern_lists_the_option_once_in_help() {
    // The separate-word branch is hidden, so help shows only the `argument`
    // branch: one `--since=DAYS` line, not two alternatives.
    let e = app()
        .parse(cli! { recent "--help" })
        .err()
        .expect("help short-circuits");
    let help = e.unwrap_stdout();
    // Count option-list entries, not mentions: the usage line names the option
    // too, and that is not a duplicate.
    let entries: Vec<&str> = help
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("--since"))
        .collect();
    assert_eq!(entries.len(), 1, "help:\n{help}");
    assert!(entries[0].starts_with("--since=DAYS"), "help:\n{help}");
}
