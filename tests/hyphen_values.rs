//! Regression tests for an option whose *value* starts with a hyphen.
//! mdma-cli spells relative dates as `--added -7`, which clap only accepts with
//! `allow_hyphen_values`.
//!
//! bpaf splits `-7` into a short flag *before* any parser runs, so with a plain
//! `argument` the glued form `--added=-7` parses as-is and the two-word form
//! `--added -7` is a usage error whose message names the glued form. #5
//! documented an `any` + `adjacent` recipe that accepted both; #29 replaced it:
//! the glued form is the only supported spelling, which keeps the standard
//! rule that a word starting with `-` is an option.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{pure, Bpaf};
use hicli::{app, cli, command, exit_code_for, App, Arg, Failure};

/// Records what ran, so a test can assert the parsed value.
pub struct Ctx {
    log: Rc<RefCell<Vec<String>>>,
}

/// A relative date: a plain long option.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Added {
    #[bpaf(long("added"), argument("DAYS"))]
    added: String,
}

/// list by date added
#[command]
fn list(a: Added, ctx: &Ctx) {
    ctx.log.borrow_mut().push(format!("list:{}", a.added));
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [list] }
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

#[test]
fn the_glued_form_parses() {
    let (code, ran) = run(&["list", "--added=-7"]);
    assert_eq!(code, 0);
    assert_eq!(ran, ["list:-7"]);
}

#[test]
fn the_two_word_form_is_a_usage_error_naming_the_glued_form() {
    let e = app()
        .parse(cli! { list "--added" "-7" })
        .err()
        .expect("expected a parse failure");
    assert_eq!(exit_code_for(&e), 2);
    assert_eq!(
        e.unwrap_stderr(),
        "`--added` requires an argument `DAYS`, got a flag `-7`, \
         try `--added=-7` to use it as an argument"
    );
    let (code, ran) = run(&["list", "--added", "-7"]);
    assert_eq!(code, 2);
    assert!(ran.is_empty(), "{ran:?}");
}

#[test]
fn an_ordinary_value_still_takes_two_words() {
    let (code, ran) = run(&["list", "--added", "7"]);
    assert_eq!(code, 0);
    assert_eq!(ran, ["list:7"]);
}

#[test]
fn help_lists_the_option_once_as_glued() {
    let e = app()
        .parse(cli! { list "--help" })
        .err()
        .expect("help short-circuits");
    let help = e.unwrap_stdout();
    // Count option-list entries, not mentions: the usage line names the option
    // too, and that is not a duplicate.
    let entries: Vec<&str> = help
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("--added"))
        .collect();
    assert_eq!(entries.len(), 1, "help:\n{help}");
    assert!(entries[0].starts_with("--added=DAYS"), "help:\n{help}");
}
