# ADR-009: Completion scripts are rendered by hicli

## Status

Accepted (2026-11-03)

Resolves johlrogge/hicli#12. Builds on ADR-006 (completion belongs to the
argument type; `App::complete` pins revision 1 because revision 2 exits) and
ADR-003 (the exit code is a value; hicli never calls `std::process::exit`).

## Decision

### 1. `completion_script(Shell, bin) -> String`

```rust
#[non_exhaustive]
pub enum Shell { Bash, Zsh, Fish }

pub fn completion_script(shell: Shell, bin: &str) -> String;
```

A free function: the script depends only on the shell and the binary's name,
never on the command tree — the tree is consulted at TAB time, when the
installed script calls the binary back. `Shell` implements `FromStr`
(`"bash"`/`"zsh"`/`"fish"`, error type `UnknownShell: std::error::Error`) and
`Display`, the same spelling both ways.

### 2. hicli owns copies of bpaf's templates

The bash, zsh and fish scripts are copied from `bpaf` 0.9.27's
`src/complete_run.rs` (MIT/Apache-2.0, attributed in the source), with the
binary name filled in. Each requests the protocol revision `bpaf` renders for
that shell: **bash 8, zsh 7, fish 9**. The runtime half — the binary answering
`--bpaf-complete-rev=N` — already works through hicli: `App::parse` returns it
as `ParseFailure::Completion` and `App::run` prints it.

### 3. `bpaf`'s `--bpaf-complete-style-*` stays unwired

`bpaf` can print these scripts itself on `--bpaf-complete-style-bash` and
friends, but only when it was handed the program name, and then it
`println!`s the script and calls `std::process::exit(0)`. hicli never hands it
the program name, so through `App::run` those flags are unknown flags — a usage
error, exit code 2. This is deliberate and pinned by tests.

### 4. The binary name is explicit

`bin` is a parameter, not `argv[0]`. The docs recommend
`env!("CARGO_BIN_NAME")`. It is pasted unquoted into shell code, so
`completion_script` **panics** if it is empty or contains anything outside
`[A-Za-z0-9._+-]` — a programming error, since it is a constant in practice.

### 5. No `Arg` for `Shell`, no built-in command

Per ADR-006, hicli ships no `Arg` impls. A consumer writes a newtype and a
hidden command of its own:

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct ShellArg(#[bpaf(positional("SHELL"))] pub hicli::Shell);

/// print the shell completion script
#[command(hide)]
fn generate_completions(s: ShellArg) {
    print!("{}", hicli::completion_script(s.0, env!("CARGO_BIN_NAME")));
}
```

The command's name, its spelling and whether `--help` lists it stay the
consumer's.

### 6. Elvish is deferred

`Shell` is `#[non_exhaustive]`, so adding `Elvish` (bpaf renders it at
revision 1) is not a breaking change. No consumer has asked for it.

### 7. Drift is caught by a tripwire test

`tests/completion_script.rs` extracts the `--bpaf-complete-rev=N` from each
generated script and drives that `N` back through `App::parse`, expecting a
`ParseFailure::Completion` with the right candidates. If a `bpaf` upgrade
renumbers a revision, `bpaf` calls `std::process::exit(1)` for the unknown
revision in debug builds — the test binary dies loudly rather than failing an
assertion, which is still a red CI.

## Why

- **The script was the missing half.** Completion already worked end to end
  except for the one artefact the user installs. Rendering it is a pure
  function of two values; owning it costs three string templates.
- **Exiting is not an option hicli has.** ADR-003 made the exit code a value
  so destructors run, `main` stays in control and the run path is testable
  in-process. bpaf's style flags violate all three; this is the same reasoning
  that pins `App::complete` to revision 1 in ADR-006.
- **`argv[0]` is the wrong name.** It may be a path, a symlink or a wrapper;
  the shell registers completion against the command word the user types,
  which only the consumer knows for certain.
- **A returned `String` is testable.** The consumer chooses where it goes —
  stdout, a file, a packaging step — and tests assert on it directly.

## Alternatives considered

- **Pass a program name to bpaf in `App::run` (`Args::set_name`)** so the style
  flags work. Rejected: bpaf then `println!`s and `process::exit(0)`s inside
  the run path — hicli would exit the process, and the path could not be tested
  in-process without killing the test runner.
- **Read the binary name from `argv[0]`.** Rejected: see *Why*; the name a
  shell completes against is not reliably the path the process was started
  with, and a path would fail the character check anyway.
- **A method on `App`.** Rejected: nothing in the script depends on the app,
  and a method would suggest otherwise.
- **Ship `impl Arg for Shell` or a built-in `generate-completions` command.**
  Rejected per ADR-006: the metavar, positional-vs-option and command name are
  the consumer's to choose.
- **Quote the binary name instead of validating it.** Rejected: each shell
  quotes differently, the name also lands inside identifiers and `#compdef`
  lines, and no real binary name needs it.

## Consequences

- New public API: `hicli::completion_script`, `hicli::Shell`,
  `hicli::UnknownShell`.
- hicli's templates must track bpaf's. A bpaf upgrade that changes a
  revision number or the script protocol is caught by the tripwire test, and
  the fix is re-copying the templates from that bpaf release.
- `--bpaf-complete-style-*` remains a usage error through `App::run`, and that
  is pinned as intended behaviour rather than a gap.
