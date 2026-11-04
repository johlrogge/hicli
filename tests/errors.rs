//! ADR-012: a command's error converts through `Into<Failure>`.
//!
//! Each command below returns the error type its author would reach for —
//! `eyre::Result`, `anyhow::Result`, `io::Result`, `Box<dyn Error>` — written
//! exactly as it would be without hicli, and is run in-process through
//! `App::run_with_args` (exit codes) and `App::parse` → `Invocation::run`
//! (the message hicli prints).

use std::error::Error;
use std::io;

use hicli::bpaf::pure;
use hicli::{app, cli, command, App, Failure, Outcome, Success};

/// The shared context. Nothing to share: the errors are the subject.
struct Ctx;

/// An `mk_ctx` for an app whose globals are `()`.
fn mk_ctx(_: ()) -> Result<Ctx, Failure> {
    Ok(Ctx)
}

/// The innermost failure every chain below starts from.
fn read_root() -> io::Result<()> {
    Err(io::Error::other("root"))
}

/// sync with eyre, failing two contexts deep
#[command]
fn eyre_sync(_ctx: &Ctx) -> eyre::Result<()> {
    // Imported here, not at the top: anyhow's `Context` and eyre's `WrapErr`
    // both add `.context` to `Result`, and would be ambiguous side by side.
    use eyre::WrapErr as _;
    read_root().wrap_err("mid").wrap_err("outer")?;
    Ok(())
}

/// succeed with eyre
#[command]
fn eyre_ok(_ctx: &Ctx) -> eyre::Result<()> {
    Ok(())
}

/// count with anyhow, finishing with a code of its own
#[command]
fn anyhow_count(_ctx: &Ctx) -> anyhow::Result<Success> {
    Ok(Success::code(7))
}

/// fail with anyhow, two contexts deep
#[command]
fn anyhow_fail(_ctx: &Ctx) -> anyhow::Result<Success> {
    use anyhow::Context as _;
    read_root().context("mid").context("outer")?;
    Ok(Success::OK)
}

/// fail with a plain io error
#[command]
fn io_fail(_ctx: &Ctx) -> io::Result<()> {
    Err(io::Error::other("disk full"))
}

/// fail with a boxed error
#[command]
fn boxed_fail(_ctx: &Ctx) -> Result<(), Box<dyn Error>> {
    Err("boxed".into())
}

/// fail with `?` on an io error inside an `Outcome`
#[command]
fn outcome_io(_ctx: &Ctx) -> Outcome {
    read_root()?;
    Ok(())
}

/// fail as a usage error, which must keep its code
#[command]
fn misuse(_ctx: &Ctx) -> Outcome {
    Err(Failure::usage("misused"))
}

fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [
            eyre_sync,
            eyre_ok,
            anyhow_count,
            anyhow_fail,
            io_fail,
            boxed_fail,
            outcome_io,
            misuse,
        ],
    }
}

/// The failure a command reports, through the same run path `App::run` uses.
fn failure_of(args: &[&str]) -> Failure {
    let (_globals, invocation) = app().parse(args.iter().copied()).expect("parses");
    invocation.run(&Ctx).expect_err("the command fails")
}

// --- exit codes ---------------------------------------------------------------

#[test]
fn an_eyre_error_exits_one() {
    assert_eq!(app().run_with_args(cli! { "eyre-sync" }, mk_ctx), 1);
}

#[test]
fn an_eyre_success_exits_zero() {
    assert_eq!(app().run_with_args(cli! { "eyre-ok" }, mk_ctx), 0);
}

#[test]
fn an_anyhow_success_carries_its_code() {
    assert_eq!(app().run_with_args(cli! { "anyhow-count" }, mk_ctx), 7);
}

#[test]
fn an_anyhow_error_exits_one() {
    assert_eq!(app().run_with_args(cli! { "anyhow-fail" }, mk_ctx), 1);
}

#[test]
fn an_io_error_exits_one() {
    assert_eq!(app().run_with_args(cli! { "io-fail" }, mk_ctx), 1);
}

#[test]
fn a_boxed_error_exits_one() {
    assert_eq!(app().run_with_args(cli! { "boxed-fail" }, mk_ctx), 1);
}

#[test]
fn question_mark_in_an_outcome_exits_one() {
    assert_eq!(app().run_with_args(cli! { "outcome-io" }, mk_ctx), 1);
}

#[test]
fn a_usage_failure_still_exits_two() {
    assert_eq!(app().run_with_args(cli! { misuse }, mk_ctx), 2);
}

// --- messages -----------------------------------------------------------------

#[test]
fn an_eyre_chain_renders_outer_to_root() {
    let f = failure_of(&["eyre-sync"]);
    assert_eq!(f.message(), "outer: mid: root");
    assert_eq!(f.code(), 1);
}

#[test]
fn an_anyhow_chain_renders_outer_to_root() {
    let f = failure_of(&["anyhow-fail"]);
    assert_eq!(f.message(), "outer: mid: root");
    assert_eq!(f.code(), 1);
}

#[test]
fn simple_errors_render_their_display() {
    assert_eq!(failure_of(&["io-fail"]).message(), "disk full");
    assert_eq!(failure_of(&["boxed-fail"]).message(), "boxed");
    assert_eq!(failure_of(&["outcome-io"]).message(), "root");
}

#[test]
fn a_usage_failure_is_not_rewrapped() {
    assert_eq!(failure_of(&["misuse"]), Failure::usage("misused"));
}

// --- the function as declared -------------------------------------------------

#[test]
fn the_function_still_returns_its_own_error_type() {
    // `#[command]` leaves the function untouched (ADR-005): called directly it
    // returns the `eyre::Report` it was written to return, not a `Failure`.
    let report: eyre::Report = eyre_sync(&Ctx).expect_err("fails");
    assert_eq!(report.to_string(), "outer");
    assert_eq!(format!("{report:#}"), "outer: mid: root");
}
