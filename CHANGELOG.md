# Changelog

All notable changes to `hicli` and `hicli-macros` are documented here. The two
crates are released together, at the same version.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Before 1.0, a minor version may carry breaking changes; they are listed under
"Breaking" with what to change.

## [Unreleased]

### Added

- A default feature, `color`: `App::run` prints help and errors in colour when stdout
  and stderr are both terminals. `NO_COLOR` turns it off and `FORCE_COLOR` forces it
  on; `hicli = { version = "0.4", default-features = false }` never colours; bpaf's
  `bright-color` feature gives bright colours. `App::parse`, `App::complete` and
  `App::into_parser` are unchanged and never colour (#31, ADR-015).

### Changed (output, not API)

`App::run` and `App::run_with_args` now print through bpaf's `print_message`, its
only coloured path (#31, ADR-015). Piped, or with `NO_COLOR`, the text changes in
three ways:

- Usage errors **and command failures** start with `Error: `, so the two look the
  same:

  ```
  $ playr playbak stop
  Error: no such command or positional: `playbak`, did you mean `playback`?
  $ playr upload missing.flac
  Error: uploading missing.flac: reading the file: No such file or directory (os error 2)
  ```

  A `Failure`'s message is still printed line for line as written, with its indents,
  but a trailing newline is dropped and a line longer than 100 columns now wraps.
- `--help` and `--version` are followed by a blank line.
- hicli's `did you mean` line is part of bpaf's error, so on a terminal the suggested
  name is styled like the rest of it.

`App::parse` returns the same messages as before, without the `Error: ` prefix, so
snapshot tests built on it need no re-blessing.

### Changed (docs)

- The `Arg` docs no longer teach accepting `--since -7`. A value that starts with `-`
  is written glued, `--since=-7`, and a plain `argument` reads it; the two-word form
  is a usage error, and bpaf's message says to write `--since=-7` instead. This keeps
  the standard rule that a word starting with `-` is an option. Code that copied the
  `any` + `adjacent` recipe keeps working; to follow the convention, replace it with
  a plain `argument` (#29).

## [0.3.0] — 2026-11-05

No breaking API changes. The signature of `hicli::group_args`, flagged in 0.2.0 as
"may change in 0.3", is unchanged. Help and error text changed, which is listed below
because snapshot tests of help pages need re-blessing.

### Added

- `App::bin_name(name)`: the program name usage lines start with. `App::run`
  defaults it to the file name of `argv[0]`. It is display only and never passed to
  bpaf (#21, ADR-013).

### Fixed

- Usage lines name the binary: `Usage: playr playback stop` (#21).
- No trailing space after a command without arguments: `Usage: playr playback stop`,
  not `Usage: playr playback stop ` (#28).
- A group's default command without options shows its help line under `Usage:`, and
  the usage reads `[COMMAND ...]`: `Usage: playr library archive [COMMAND ...]`, then
  `List the archive` (#19).
- A curried value's field doc is listed under "Available positional items:", not as a
  row of "Commands for VALUE:" (#20).
- A curried group given a value but no command says
  ``expected a command after `bandcamp`: `check-updates` `` instead of blaming the
  value, and a typo there gets a "did you mean":
  ``no such command after `bandcamp`: `snyc`, did you mean `sync`?`` (#27). This also
  closes ADR-011's known limit that a typo after the value got no suggestion.

### Changed (help text, not API)

- Every `Usage:` line in `App::run` output now starts with the binary's name.
  `App::parse` and `App::run_with_args` print no binary name unless `.bin_name(..)` is
  set. Re-bless snapshot tests of help pages; set `.bin_name(..)` in them to match what
  `run` prints.
- The `--help` page of a group whose default command has no options gains the
  default's help line under `Usage:` (and the group's details follow it), and its
  usage line reads `[COMMAND ...]` instead of `COMMAND ...` (#19).
- The `--help` page of a curried group lists the value's row under "Available
  positional items:" (or its type's doc heading) instead of under "Commands for
  VALUE:" (#20).
- The error for a missing or misspelled curried command, after the value:
  ``expected a command after `VALUE`: …`` or ``no such command after `VALUE`: …, did
  you mean …?``, where it was ``expected `COMMAND ...`, got `VALUE` `` (#27).

## [0.2.1] — 2026-11-05

### Fixed

- A plain `mod helper;` inside a file group (`#[group] mod source;`) resolves to
  `source/helper.rs`, as in any Rust file, instead of failing with E0583 (#25), at
  every nesting level. `#[group]` now emits `#[path]` only where rustc's own rules
  would not find the group's file: under a crate root not named `lib.rs`/`main.rs`
  that keeps its modules in a same-named directory (`tests/cli.rs` + `tests/cli/`),
  inside a file loaded through `#[path]`, or when both `x.rs` and `x/mod.rs` exist.
  In those files rustc resolves plain submodules beside the file; give them a
  `#[path]`. An existing `#[path = "source/import.rs"]` workaround keeps working and
  can be removed. If you instead moved a helper module *beside* the group file to
  get around this, move it into the group's directory (or give it a `#[path]`).

## [0.2.0] — 2026-11-04

### Added

- **Curried groups** (ADR-011, #1): `#[group(args = T)]` parses one `T` after the
  group's name and hands it to its `#[command(curried)]` children (curried over their
  first owned parameter). `#[group(curried = T)]` is a group nested after that value.
  Unmarked children are fixed siblings and are matched first. Hand-written forms:
  `hicli::group_args`, `hicli::curried_command`, `hicli::Curried`.
- **A group's default command** (ADR-011, #4): `#[command(default)]` /
  `Node::by_default` runs when no sibling is named. It answers to no word of its own,
  and its `Invocation::path` is the group's.
- **`?` on any error** (ADR-012, #6): a command may return `io::Result<()>`,
  `anyhow::Result<()>`, `color_eyre::Result<()>`, `Result<(), Box<dyn Error>>`,
  `Result<(), String>`, … as written. Any error becomes `Failure::Runtime` (exit 1),
  rendered as its source chain `outer: mid: root`. `?` also works inside
  `Outcome`-returning commands, and inside `mk_ctx`, which still returns
  `Result<Ctx, Failure>`.
- Fixed: `App::complete` dropped the first candidate; debug builds now catch a
  positional declared before an options struct at fold time (it panicked on `--help`).

### Breaking — upgrading from 0.1

1. A hand-built closure passed to `hicli::command`/`curried_command` that only returns
   `Ok(())` needs `-> Outcome` spelled (E0282 otherwise). `#[command]` functions are
   unaffected.
2. Delete any `impl From<YourError> for hicli::Failure` where
   `YourError: std::error::Error`. It now conflicts (E0119) with hicli's blanket
   conversion, which does the same job and renders the full chain. For a hand-chosen
   message use `.map_err(Failure::runtime)`; for exit code 2 return
   `Failure::usage(..)`; errors are never downcast to a code.
3. `Command<Ctx>` is now an alias for `Node<Invocation<Ctx>>`, and `hicli::group` is
   generic over the node's output (`group<O>`). A turbofish `hicli::group::<Ctx>(…)`
   must be dropped or written `group::<Invocation<Ctx>>(…)`.
4. `Result<(), Box<dyn Error + Send + Sync>>` is not an accepted return type. Return
   `Box<dyn Error>`, `anyhow::Error` or `eyre::Report`, or map with `Failure::runtime`.
5. MSRV unchanged at 1.99; `hicli` and `hicli-macros` must be the same version (exact
   pin).

### May change in 0.3

- The signature of the low-level `hicli::group_args` (`#[group(args = T)]` is the
  documented path).

## [0.1.0] — 2026-11-03

First release: help metadata and the doc comment's `# Examples` footer (ADR-008),
completion scripts (ADR-009) and groups in files (ADR-010).

[0.3.0]: https://github.com/johlrogge/hicli/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/johlrogge/hicli/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/johlrogge/hicli/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/johlrogge/hicli/releases/tag/v0.1.0
