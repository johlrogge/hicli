//! Shell completion scripts (issue #12, ADR-009).
//!
//! Two halves, and hicli owns both ends in-process:
//!
//! 1. **The script** — [`hicli::completion_script`] renders it from hicli's own
//!    copies of `bpaf`'s bash/zsh/fish templates. `bpaf`'s
//!    `--bpaf-complete-style-*` flags, which would print the script and call
//!    `std::process::exit(0)`, are deliberately **not** wired: through hicli
//!    they are unknown flags, a usage error. The tests below pin that.
//!
//! 2. **The runtime the script calls back into** — the installed scripts invoke
//!    the binary with `--bpaf-complete-rev=8` / `=7` / `=9`, which come back as a
//!    `ParseFailure::Completion` value (no exit). The tripwire tests extract the
//!    revision from each generated script and drive it back through
//!    `App::parse`, so the two halves cannot drift apart silently.
//!
//! Everything here is in-process; nothing spawns a subprocess.

use hicli::bpaf::{pure, Bpaf, ParseFailure};
use hicli::{app, cli, command, completion_script, exit_code_for, App, Arg, Candidate, Shell};

/// The shared context: nothing here runs a command, only parses and completes.
pub struct Ctx;

/// A person's id.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_ids)]
pub struct Id(#[bpaf(positional("ID"))] String);

/// The ids on offer, filtered by what has been typed so far.
fn complete_ids(typed: &Id) -> Vec<Candidate> {
    ["alice", "alfred"]
        .iter()
        .filter(|c| c.starts_with(typed.0.as_str()))
        .map(|c| ((*c).to_string(), Some("a person".to_string())))
        .collect()
}

/// A directory, completed by the shell itself.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]
pub struct Home(#[bpaf(positional("DIR"))] String);

/// show a person
#[command]
pub fn show(id: Id, ctx: &Ctx) {
    let _ = (id, ctx);
}

/// store things somewhere
#[command]
pub fn store(home: Home, ctx: &Ctx) {
    let _ = (home.0, ctx);
}

// The recipe from `completion_script`'s docs, compiled for real. Its doc
// comment's first paragraph is its help row.

/// A shell to print completions for
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SHELL")]
pub struct ShellArg(pub hicli::Shell);

/// print the shell completion script
#[command(hide)]
fn generate_completions(s: ShellArg) {
    // `env!("CARGO_BIN_NAME")` in a binary crate; an integration test has none.
    print!("{}", completion_script(s.0, "mdma"));
}

/// The whole tree.
fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [show, store, generate_completions],
    }
}

const SHELLS: [Shell; 3] = [Shell::Bash, Shell::Zsh, Shell::Fish];

// ---------------------------------------------------------------------------
// `completion_script`: the rendered scripts.
// ---------------------------------------------------------------------------

#[test]
fn every_script_names_the_binary_and_requests_its_revision() {
    for (shell, rev) in [(Shell::Bash, 8), (Shell::Zsh, 7), (Shell::Fish, 9)] {
        let script = completion_script(shell, "mdma");
        assert!(script.contains("mdma"), "{shell}: got:\n{script}");
        assert!(
            script.contains(&format!("--bpaf-complete-rev={rev}")),
            "{shell}: expected revision {rev}, got:\n{script}"
        );
        assert!(script.ends_with('\n'), "{shell}: no trailing newline");
    }
}

#[test]
fn the_bash_script_registers_the_binary() {
    let script = completion_script(Shell::Bash, "mdma");
    assert!(
        script.contains("complete -o nosort -F _bpaf_dynamic_completion mdma\n"),
        "got:\n{script}"
    );
}

#[test]
fn the_zsh_script_is_a_compdef_for_the_binary() {
    let script = completion_script(Shell::Zsh, "mdma");
    assert!(script.starts_with("#compdef mdma\n"), "got:\n{script}");
}

#[test]
fn the_fish_script_registers_the_binary() {
    let script = completion_script(Shell::Fish, "mdma");
    assert!(
        script.contains(
            "complete --no-files --command mdma --arguments '(_bpaf_dynamic_completion)'"
        ),
        "got:\n{script}"
    );
}

#[test]
fn a_name_with_every_allowed_character_is_accepted() {
    for shell in SHELLS {
        let script = completion_script(shell, "my-tool_2.0+x");
        assert!(script.contains("my-tool_2.0+x"), "{shell}: got:\n{script}");
    }
}

#[test]
#[should_panic(expected = "must be non-empty")]
fn an_empty_binary_name_panics() {
    let _ = completion_script(Shell::Bash, "");
}

#[test]
#[should_panic(expected = "pasted unquoted into shell code")]
fn a_binary_name_with_a_space_panics() {
    let _ = completion_script(Shell::Zsh, "my tool");
}

#[test]
#[should_panic(expected = "pasted unquoted into shell code")]
fn a_binary_name_that_is_a_path_panics() {
    let _ = completion_script(Shell::Fish, "/usr/bin/mdma");
}

#[test]
#[should_panic(expected = "pasted unquoted into shell code")]
fn a_binary_name_with_shell_syntax_panics() {
    let _ = completion_script(Shell::Bash, "mdma;rm");
}

// ---------------------------------------------------------------------------
// `Shell`: FromStr / Display.
// ---------------------------------------------------------------------------

#[test]
fn shell_round_trips_through_its_name() {
    for (shell, name) in [
        (Shell::Bash, "bash"),
        (Shell::Zsh, "zsh"),
        (Shell::Fish, "fish"),
    ] {
        assert_eq!(shell.to_string(), name);
        assert_eq!(name.parse::<Shell>().expect("a known shell"), shell);
    }
}

#[test]
fn an_unknown_shell_is_an_error_naming_it() {
    // `Shell` is a `Choice`: the error lists the names and leaves the typed
    // word to bpaf's ``couldn't parse `tcsh`: `` in front of it.
    for (name, want) in [
        // Two edits from both `bash` and `zsh`: the tie goes to the earlier.
        (
            "tcsh",
            "expected `bash`, `zsh` or `fish`; did you mean `bash`?",
        ),
        ("elvish", "expected `bash`, `zsh` or `fish`"),
        ("", "expected `bash`, `zsh` or `fish`"),
        ("powershell", "expected `bash`, `zsh` or `fish`"),
        (
            "Bash",
            "expected `bash`, `zsh` or `fish`; did you mean `bash`?",
        ),
        (
            "fishh",
            "expected `bash`, `zsh` or `fish`; did you mean `fish`?",
        ),
    ] {
        let e = name.parse::<Shell>().expect_err("not a known shell");
        assert_eq!(e.to_string(), want, "{name:?}");
        assert_eq!(e.typed(), name);
        // It is a real error type, usable behind `?` in a `Box<dyn Error>`.
        let _: Box<dyn std::error::Error> = Box::new(e);
    }
}

#[test]
#[allow(deprecated)]
fn unknown_shell_is_still_the_shell_error_type() {
    let e: hicli::UnknownShell = "tcsh".parse::<Shell>().expect_err("not a known shell");
    let _: hicli::UnknownChoice = e;
}

// ---------------------------------------------------------------------------
// The documented wiring: a `ShellArg` newtype and a hidden command.
// ---------------------------------------------------------------------------

#[test]
fn the_documented_shell_arg_completes_the_shells() {
    // `Shell` is a `Choice`, so the spelled newtype completes with no
    // completer of its own.
    assert_eq!(
        app().complete(cli! { "generate-completions" "" }),
        ["bash", "zsh", "fish"]
    );
    assert_eq!(app().complete(cli! { "generate-completions" f }), ["fish"]);
}

#[test]
fn the_documented_generate_completions_command_parses_and_runs() {
    let (_g, invocation) = app()
        .parse(cli! { "generate-completions" zsh })
        .expect("parses");
    assert_eq!(invocation.path(), ["generate-completions"]);
    assert_eq!(
        app().run_with_args(cli! { "generate-completions" zsh }, |_| Ok(Ctx)),
        0
    );
}

#[test]
fn an_unknown_shell_is_a_usage_error_naming_the_choices() {
    match app().parse(cli! { "generate-completions" tcsh }) {
        Err(e @ ParseFailure::Stderr(..)) => {
            assert_eq!(exit_code_for(&e), 2);
            assert_eq!(
                e.unwrap_stderr(),
                "couldn't parse `tcsh`: `SHELL`: expected `bash`, `zsh` or `fish`; did you mean `bash`?"
            );
        }
        Err(_) => panic!("not a usage error"),
        Ok(_) => panic!("an unknown shell parsed"),
    }
}

#[test]
fn the_generate_completions_command_is_hidden() {
    let help = app()
        .parse(cli! { --help })
        .err()
        .expect("--help short-circuits")
        .unwrap_stdout();
    assert!(help.contains("show"), "got:\n{help}");
    assert!(
        !help.contains("generate-completions"),
        "a hidden command is listed:\n{help}"
    );
    assert!(!app()
        .complete(cli! { "" })
        .contains(&"generate-completions".to_string()));
}

// ---------------------------------------------------------------------------
// `bpaf`'s style flags: unwired by design.
// ---------------------------------------------------------------------------

/// The script-generation flags `bpaf` 0.9.27 understands for the three shells
/// (`complete_run.rs`). It prints the script and calls `process::exit(0)` —
/// but only when handed a program name, which hicli never does.
const STYLE_FLAGS: [&str; 3] = [
    "--bpaf-complete-style-bash",
    "--bpaf-complete-style-zsh",
    "--bpaf-complete-style-fish",
];

#[test]
fn style_flags_are_not_wired_by_design() {
    // `App::parse` hands bpaf a nameless `&[&str]`, exactly as
    // `run_with_args` does, so this is what `run`'s parse step sees: an unknown
    // flag, not a script and not an exit.
    for flag in STYLE_FLAGS {
        match app().parse([flag]) {
            Err(ParseFailure::Stderr(msg)) => {
                let text = msg.to_string();
                assert!(
                    text.contains(flag),
                    "{flag}: expected a usage error naming the flag, got:\n{text}"
                );
            }
            Err(ParseFailure::Stdout(..)) => panic!("{flag}: unexpectedly rendered as help"),
            Err(ParseFailure::Completion(out)) => {
                panic!("{flag}: unexpectedly treated as completion:\n{out}")
            }
            Ok(_) => panic!("{flag}: unexpectedly parsed"),
        }
    }
}

#[test]
fn style_flags_through_run_with_args_are_a_usage_error_by_design() {
    // The whole run pipeline: no script, no exit, exit code 2. (The error text
    // goes to the real stderr — `run_with_args` does not capture it.)
    for flag in STYLE_FLAGS {
        let code = app().run_with_args([flag], |_| Ok(Ctx));
        assert_eq!(code, 2, "{flag}: expected a usage error");
    }
}

// ---------------------------------------------------------------------------
// The runtime protocol the installed scripts call back into.
// ---------------------------------------------------------------------------

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

/// The `N` in the `--bpaf-complete-rev=N` a generated script calls back with.
fn requested_revision(script: &str) -> usize {
    const MARK: &str = "--bpaf-complete-rev=";
    let at = script.find(MARK).expect("the script requests a revision") + MARK.len();
    let digits: String = script[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().expect("a numeric revision")
}

#[test]
fn every_generated_script_requests_a_revision_bpaf_answers() {
    // The tripwire. hicli's templates are copies of bpaf's, so a bpaf upgrade
    // that renumbers a revision would leave hicli shipping a script that asks
    // for one bpaf no longer renders. Driving each script's own revision back
    // through `App::parse` is what catches that.
    //
    // It does not fail *cleanly*: for an unknown revision bpaf calls
    // `std::process::exit(1)` in debug builds (and `exit(0)` in release), so
    // the test binary dies loudly mid-run instead of reporting a failed
    // assertion. That is still a red CI, which is the point.
    for shell in SHELLS {
        let rev = requested_revision(&completion_script(shell, "mdma"));
        let out = complete_at(rev, &["show", "al"]);
        assert!(
            out.contains("alice") && out.contains("alfred"),
            "{shell} (rev {rev}): got:\n{out}"
        );
    }
}

#[test]
fn bash_runtime_revision_8_completes_in_process() {
    // The bash script runs `$1 --bpaf-complete-rev=8 ${COMP_WORDS[@]:1}` and
    // `source`s the output.
    let out = complete_at(8, &[""]);
    assert!(out.contains("COMPREPLY+=("), "got:\n{out}");
    assert!(out.contains("show") && out.contains("store"), "got:\n{out}");

    let out = complete_at(8, &["show", "al"]);
    assert!(
        out.contains("alice") && out.contains("alfred"),
        "got:\n{out}"
    );
}

#[test]
fn zsh_runtime_revision_7_completes_in_process() {
    let out = complete_at(7, &[""]);
    assert!(out.contains("compadd"), "got:\n{out}");
    assert!(out.contains("show") && out.contains("store"), "got:\n{out}");

    let out = complete_at(7, &["show", "al"]);
    assert!(
        out.contains("alice") && out.contains("alfred"),
        "got:\n{out}"
    );
}

#[test]
fn fish_runtime_revision_9_completes_in_process() {
    // fish reads plain `value\tdescription` lines.
    let out = complete_at(9, &["show", "al"]);
    let values: Vec<&str> = out.lines().filter_map(|l| l.split('\t').next()).collect();
    assert!(values.contains(&"alice"), "got:\n{out}");
    assert!(values.contains(&"alfred"), "got:\n{out}");
}

#[test]
fn shell_directives_survive_the_shell_revisions() {
    // What `App::complete` (revision 1) cannot show: `complete = dirs` becomes
    // a real shell instruction at the bash/zsh revisions.
    let bash = complete_at(8, &["store", ""]);
    assert!(bash.contains("_filedir -d"), "bash got:\n{bash}");

    let zsh = complete_at(7, &["store", ""]);
    assert!(zsh.contains("_files -/"), "zsh got:\n{zsh}");
}
