//! Installable shell completion scripts: [`completion_script`] and [`Shell`].
//!
//! A completion script is the small piece of shell code a user installs once
//! (in `~/.bash_completion.d/`, on `$fpath`, in `~/.config/fish/completions/`).
//! It does not know the command tree: on every TAB it calls the binary back with
//! `--bpaf-complete-rev=N` and the words typed so far, and `bpaf` answers in the
//! shell's own syntax. That runtime half already works through hicli —
//! [`App::parse`](crate::App::parse) returns it as a
//! [`ParseFailure::Completion`](bpaf::ParseFailure::Completion) value and
//! [`App::run`](crate::App::run) prints it — so the only thing missing was the
//! script itself, which is what this module renders. See ADR-009.

use std::fmt;
use std::str::FromStr;

/// A shell [`completion_script`] can render a script for.
///
/// `#[non_exhaustive]` so a shell can be added later (elvish is the obvious
/// candidate) without a breaking change. A `match` on it outside this crate
/// needs a wildcard arm.
///
/// It parses from and displays as its lowercase name — `"bash"`, `"zsh"`,
/// `"fish"` — which is what a `generate-completions bash` command line carries:
///
/// ```
/// use hicli::Shell;
///
/// let s: Shell = "zsh".parse().expect("a known shell");
/// assert_eq!(s, Shell::Zsh);
/// assert_eq!(s.to_string(), "zsh");
/// assert!("tcsh".parse::<Shell>().is_err());
/// ```
///
/// `hicli` ships no [`Arg`](trait@crate::Arg) impl for `Shell` (ADR-006): the
/// metavar and positional-vs-option are yours to name. See
/// [`completion_script`] for the newtype to write.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shell {
    /// GNU bash.
    Bash,
    /// zsh.
    Zsh,
    /// fish.
    Fish,
}

impl Shell {
    /// The shell's lowercase name, as [`FromStr`] reads it.
    fn name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
        }
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The error from parsing a [`Shell`] out of a name it does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownShell(String);

impl fmt::Display for UnknownShell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown shell `{}`: expected one of `bash`, `zsh`, `fish`",
            self.0
        )
    }
}

impl std::error::Error for UnknownShell {}

impl FromStr for Shell {
    type Err = UnknownShell;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bash" => Ok(Shell::Bash),
            "zsh" => Ok(Shell::Zsh),
            "fish" => Ok(Shell::Fish),
            other => Err(UnknownShell(other.to_string())),
        }
    }
}

/// The installable completion script for `shell`, completing the binary `bin`.
///
/// ```
/// use hicli::{completion_script, Shell};
///
/// let script = completion_script(Shell::Bash, "mdma");
/// assert!(script.contains("complete -o nosort -F _bpaf_dynamic_completion mdma"));
/// assert!(script.contains("--bpaf-complete-rev=8"));
/// ```
///
/// The script calls the binary back with `--bpaf-complete-rev=N` — bash `8`,
/// zsh `7`, fish `9` — which [`App::run`](crate::App::run) already answers.
/// The templates are hicli's own copies of `bpaf` 0.9.27's, so the revisions
/// they request are exactly the ones the linked `bpaf` renders; a test drives
/// each script's revision back through [`App::parse`](crate::App::parse) so a
/// `bpaf` upgrade that renumbers them fails loudly rather than shipping a dead
/// script.
///
/// # The binary name is a parameter
///
/// Not `argv[0]`: that may be a path, a symlink or a wrapper, and the shell
/// registers completion for the **command name** the user types. Pass the name
/// the binary is installed as — `env!("CARGO_BIN_NAME")` in a binary crate is
/// the usual answer.
///
/// # Wiring it to a command
///
/// `hicli` ships no [`Arg`](trait@crate::Arg) for [`Shell`] (ADR-006), and no
/// built-in command: a consumer writes a newtype and a hidden command of its
/// own, which keeps the command's name, its spelling and whether it is listed
/// in `--help` theirs to choose. Something like:
///
/// ```text
/// /// A shell to generate completions for.
/// #[derive(Debug, Clone, Bpaf, Arg)]
/// pub struct ShellArg(#[bpaf(positional("SHELL"))] pub hicli::Shell);
///
/// /// print the shell completion script
/// #[command(hide)]
/// fn generate_completions(s: ShellArg) {
///     print!("{}", hicli::completion_script(s.0, env!("CARGO_BIN_NAME")));
/// }
/// ```
///
/// (`tests/completion_script.rs` compiles and runs exactly this, with a literal
/// binary name), and the user installs it with, for example,
/// `mdma generate-completions bash > ~/.local/share/bash-completion/completions/mdma`.
///
/// # Why not `bpaf`'s `--bpaf-complete-style-*`
///
/// `bpaf` can print these scripts itself, when it sees
/// `--bpaf-complete-style-bash` and friends — but it does so by `println!`ing
/// the script and calling `std::process::exit(0)`, and only when it was handed
/// the program name. That breaks hicli's contract that the run path never
/// exits the process (destructors run, `main` stays in control, and the run
/// path is testable in-process), for the same reason ADR-006 pins
/// [`App::complete`](crate::App::complete) to revision 1. hicli therefore never
/// hands `bpaf` the program name, and those flags stay **unwired** by design:
/// through [`App::run`](crate::App::run) they are unknown flags, a usage error.
///
/// # Panics
///
/// If `bin` is empty, or contains anything outside `[A-Za-z0-9._+-]`. The name
/// is pasted **unquoted** into shell code, so anything else — a space, a quote,
/// a `$`, a `/` — would be a broken or injectable script rather than a
/// completion. A binary name is a compile-time constant in practice, so this is
/// a programming error, not a runtime one.
pub fn completion_script(shell: Shell, bin: &str) -> String {
    assert!(
        !bin.is_empty()
            && bin
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-')),
        "completion_script: binary name {bin:?} must be non-empty and contain only [A-Za-z0-9._+-]: \
         it is pasted unquoted into shell code"
    );
    match shell {
        Shell::Bash => bash(bin),
        Shell::Zsh => zsh(bin),
        Shell::Fish => fish(bin),
    }
}

// The three templates below are copied from `bpaf` 0.9.27, `src/complete_run.rs`
// (`dump_bash_completer`, `dump_zsh_completer`, `dump_fish_completer`),
// Copyright (c) Michael Baykov, used under the MIT / Apache-2.0 licence `bpaf`
// is distributed under. `bpaf` `println!`s them, so each ends with the newline
// `println!` would have added. The revision each one requests — bash 8, zsh 7,
// fish 9 — must match what the linked `bpaf` renders; the tripwire test in
// `tests/completion_script.rs` checks it.

/// bpaf's bash completer, protocol revision 8.
fn bash(name: &str) -> String {
    format!(
        r#"_bpaf_dynamic_completion()
{{
    line="$1 --bpaf-complete-rev=8 ${{COMP_WORDS[@]:1}}"
    if [[ ${{COMP_WORDS[-1]}} == "" ]]; then
        line="${{line}} \"\""
    fi
    source <( eval ${{line}})
}}
complete -o nosort -F _bpaf_dynamic_completion {name}
"#
    )
}

/// bpaf's zsh completer, protocol revision 7.
fn zsh(name: &str) -> String {
    format!(
        r#"#compdef {name}
local line
line="${{words[1]}} --bpaf-complete-rev=7 ${{words[@]:1}}"
if [[ ${{words[-1]}} == "" ]]; then
    line="${{line}} \"\""
fi
source <(eval ${{line}})

"#
    )
}

/// bpaf's fish completer, protocol revision 9.
fn fish(name: &str) -> String {
    format!(
        r#"function _bpaf_dynamic_completion
    set -l current (commandline --tokenize --current-process)
    set -l tmpline --bpaf-complete-rev=9 $current[2..]
    if test (commandline --current-process) != (string trim (commandline --current-process))
        set tmpline $tmpline ""
    end
    eval $current[1] \"$tmpline\"
end

complete --no-files --command {name} --arguments '(_bpaf_dynamic_completion)'

"#
    )
}
