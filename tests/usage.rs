//! Usage lines (ADR-013, #21, #28): hicli renders every one itself.
//!
//! A usage line names the binary when one is known — `App::bin_name`, or under
//! `App::run` the file name of `argv[0]` — spells the command path (with every
//! curried value's metavar), then whatever bpaf derives from the command's own
//! items, joined by single spaces and never ending in one.
//!
//! Everything is in-process: `App::parse` and `App::run_with_args`.

use hicli::bpaf::{Bpaf, ParseFailure};
use hicli::{app, cli, command, group, App, Arg};

/// Global options
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// The node to talk to
    #[bpaf(long, argument("HOST"), fallback(String::from("localhost")))]
    pub node: String,
}

/// The shared context.
pub struct Ctx;

/// An audio output device.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Output(
    /// An audio output device
    #[bpaf(positional("NAME"))]
    String,
);

/// A source's name.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Source(
    /// The source
    #[bpaf(positional("SOURCE"))]
    String,
);

/// Whether to apply what was found.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Apply {
    /// Apply the updates found
    #[bpaf(long, switch)]
    apply: bool,
}

/// Check that the node answers
#[command]
fn ping(ctx: &Ctx) {
    let _ = ctx;
}

/// Playback control
#[group]
mod playback {
    use super::*;

    /// Start playback from the queue
    #[command]
    pub fn start(ctx: &Ctx) {
        let _ = ctx;
    }

    /// Stop playback
    #[command]
    pub fn stop(ctx: &Ctx) {
        let _ = ctx;
    }

    /// Select an audio output device
    #[command]
    pub fn set_output(output: Output, ctx: &Ctx) {
        let _ = ctx;
        println!("output is now {}", output.0);
    }
}

/// Music sources
#[group(args = Source)]
mod source {
    use super::*;

    /// List the sources
    #[command]
    pub fn list(ctx: &Ctx) {
        let _ = ctx;
    }

    /// Check the source for updates
    #[command(curried)]
    pub fn check_updates(source: Source, apply: Apply, ctx: &Ctx) {
        let _ = ctx;
        println!("checking {} (apply: {})", source.0, apply.apply);
    }
}

/// The tree, with no binary name.
fn nameless() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [ping, playback, source],
    }
}

/// The tree as `playr`.
fn playr() -> App<Globals, Ctx> {
    nameless().bin_name("playr")
}

/// The help page `args` prints.
fn help(app: App<Globals, Ctx>, args: &[&str]) -> String {
    match app.parse(args.iter().copied()) {
        Err(e @ ParseFailure::Stdout(..)) => e.unwrap_stdout(),
        Err(other) => panic!("{args:?}: expected help, got {other:?}"),
        Ok(_) => panic!("{args:?}: expected help, it parsed"),
    }
}

/// The `Usage:` line of a help page.
fn usage(page: &str) -> &str {
    page.lines()
        .find(|l| l.starts_with("Usage:"))
        .unwrap_or_else(|| panic!("no usage line in:\n{page}"))
}

/// Panic if any line of `page` ends in a space.
fn assert_no_trailing_space(page: &str) {
    for line in page.lines() {
        assert!(!line.ends_with(' '), "trailing space in {line:?}:\n{page}");
    }
}

#[test]
fn the_top_level_usage_names_the_binary() {
    let h = help(playr(), &["--help"]);
    assert_eq!(usage(&h), "Usage: playr [--node=HOST] COMMAND ...", "{h}");
    assert_no_trailing_space(&h);
}

#[test]
fn a_groups_usage_names_the_binary_and_the_group() {
    let h = help(playr(), &["playback", "--help"]);
    assert_eq!(usage(&h), "Usage: playr playback COMMAND ...", "{h}");
    assert_no_trailing_space(&h);
}

#[test]
fn a_command_without_arguments_has_no_trailing_space() {
    let h = help(playr(), &["playback", "stop", "--help"]);
    assert_eq!(usage(&h), "Usage: playr playback stop", "{h}");
    assert_no_trailing_space(&h);
}

#[test]
fn a_commands_arguments_follow_its_path() {
    let h = help(playr(), &["playback", "set-output", "--help"]);
    assert_eq!(usage(&h), "Usage: playr playback set-output NAME", "{h}");
    assert_no_trailing_space(&h);
}

#[test]
fn a_curried_commands_usage_names_the_binary_and_the_value() {
    let h = help(playr(), &["source", "bandcamp", "check-updates", "--help"]);
    assert_eq!(
        usage(&h),
        "Usage: playr source SOURCE check-updates [--apply]",
        "{h}"
    );
    let h = help(playr(), &["source", "list", "--help"]);
    assert_eq!(usage(&h), "Usage: playr source list", "{h}");
    assert_no_trailing_space(&h);
}

#[test]
fn without_a_bin_name_the_test_seams_show_none() {
    let h = help(nameless(), &["playback", "stop", "--help"]);
    assert_eq!(usage(&h), "Usage: playback stop", "{h}");
    assert_no_trailing_space(&h);
    let h = help(nameless(), &["--help"]);
    assert_eq!(usage(&h), "Usage: [--node=HOST] COMMAND ...", "{h}");
}

#[test]
fn a_bin_name_does_not_wire_bpafs_script_flags() {
    // ADR-009 §3: the name is display only, never `Args::set_name`, so bpaf's
    // own `--bpaf-complete-style-*` handling (print + exit) stays off.
    for flag in [
        "--bpaf-complete-style-bash",
        "--bpaf-complete-style-zsh",
        "--bpaf-complete-style-fish",
    ] {
        assert_eq!(playr().run_with_args([flag], |_| Ok(Ctx)), 2, "{flag}");
    }
}

#[test]
fn a_bin_name_changes_no_parsing() {
    let (_, inv) = playr().parse(cli! { playback stop }).expect("parses");
    assert_eq!(inv.path(), ["playback", "stop"]);
}

#[test]
fn a_hand_built_command_gets_the_same_usage_line() {
    let stop = command(
        "stop",
        "stop playback",
        hicli::bpaf::pure(()),
        |(), _: &Ctx| {},
    );
    let app = App::new(globals()).bin_name("playr").command(group(
        "playback",
        "playback control",
        vec![stop],
    ));
    let h = match app.parse(cli! { playback stop --help }) {
        Err(e) => e.unwrap_stdout(),
        Ok(_) => panic!("expected help"),
    };
    assert_eq!(usage(&h), "Usage: playr playback stop", "{h}");
}
