# Changelog

All notable changes to `hicli` and `hicli-macros` are documented here. The two
crates are released together, at the same version.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Before 1.0, a minor version may carry breaking changes; they are listed under
"Breaking" with what to change.

## [Unreleased]

### Added

- `#[derive(Arg)]` composes a struct with named fields, each an argument type. A
  field's doc comment overrides its type's help, so one type can read differently in
  each struct. The struct's doc comment is a section heading. Globals written as a
  spelled newtype or a composed struct need no `#[derive(Bpaf)]` and no direct
  `bpaf` dependency (ADR-019).
- `env = "VAR"` on a spelled option: an environment variable it falls back to, shown
  in help as `[env:VAR: N/A]`. A bad value from the environment is named with its
  variable and never echoed: ``couldn't parse: `--node` from `$PLAYR_NODE`: …``
  (ADR-019).
- `Option<T>` of an argument type, as a command's parameter or a composed struct's
  field, means "T, if given": `fn list(limit: Option<Limit>)`. `Limit` keeps its own
  `FromStr`. Everything not marked optional is required, and the derive marks it:
  `Arg::OPTIONAL`, a defaulted `false` const that `#[derive(Arg)]` sets for an
  `Option` or `Vec` inside and a switch; a composed struct's is the AND of its
  fields'. `Option` of a type that succeeds when its argument is absent is a
  compile error (E0080, `` `Added` succeeds when its argument is absent, so
  `Option<Added>` would always be `Some` … ``), checked at each use, under
  `cargo check` too, and `Option<Option<T>>` is refused. A hand-written `Arg` that
  is optional this way writes `const OPTIONAL: bool = true;` (ADR-019).

### Breaking

- A `#[derive(Bpaf, Arg)]` struct with named fields and no `#[bpaf(…)]` attribute is
  now composed and fails with E0428: compose it, or add a `#[bpaf(…)]` attribute.
- A `#[derive(Arg)]` struct with named fields and a hand-written snake_case
  constructor is now composed: delete the constructor, or write `impl Arg` by hand.
- `#[arg(…)]` on a field is a compile error. It was ignored before.
- `#[arg(complete = …)]` on such a struct reports the composed-struct message.
- `Arg`'s and `ArgHelp`'s diagnostics are reworded to cover fields and `Option<T>`:
  re-bless trybuild snapshots. Golden expansions of spelled `Option`, `Vec` and switch
  types gain `const OPTIONAL: bool = true;`.
- `playr`'s `--help` (the README's example) lists its one global under
  `Available options:`: a single global is a spelled newtype, which has no heading.

## [0.6.0] — 2026-11-09

### Added

- A struct with several fields can be spelled with `#[arg(positional|long|short …)]`
  when it implements `FromStr`: it is one word, parsed whole
  (`#[arg(positional = "FILE", complete = files)] struct Upload { path: PathBuf, zip: bool }`)
  (ADR-018).

### Changed

- A spelled one-value type parses through its own `FromStr` when it has one, else
  through its field's. In 0.5 the type's own rule was ignored: `LimitArg(NonZeroUsize)`
  didn't enforce "at least 1", and `ExistingFile(PathBuf)` didn't check that the file
  exists. In `T(Option<I>)` and `T(Vec<I>)`, each word still parses through `I`'s
  `FromStr` (ADR-018).
- A spelled parse error names the argument:
  ``couldn't parse `0`: `--limit`: must be at least 1``. An option is named by its
  long name, else its short one; a positional by its metavar. Choice errors do too
  (``couldn't parse `bmp`: `--sort`: expected `bpm` or `title`; did you mean `bpm`?``).
  Types spelled by `#[derive(Bpaf)]` and hand-written `Arg` impls are unchanged
  (#26, in part).
- A `Choice`'s help row ends with its names, for example
  `KEY  What to sort by [bpm, title, artist]`. This applies to a choice enum, a
  newtype over one, `Option`/`Vec` of one, and an `# Arguments` override. A help
  ending in a full stop gets the names before it:
  `Never played takes precedence [never].` The names are bold in colour.
- Typo suggestions (commands and choices) allow one edit for words under six letters
  and two from six, and count a swapped pair of letters as one edit: `bmp` still
  suggests `bpm`, `nope` no longer suggests `role`.

### Breaking

- Help and error snapshots change: re-bless them.
- Remove choice names that a doc comment lists by hand; they now appear twice.
- A type with its own `FromStr` may now reject words that 0.5 accepted.
- A newtype where neither the type nor its field has `FromStr` is now E0599 naming
  both bounds (it was E0277).
- Spelling keys on a multi-field struct no longer give "spelled by
  `#[derive(Bpaf)]`"; without `FromStr`, they give E0277 (`T: FromStr`).

### Documentation

- The lazy-context recipe: a context part that is expensive or fallible is a
  `OnceLock` built the first time a command asks for it; `mk_ctx` needs no access to
  the selected command (#7, ADR-014).

## [0.5.0] — 2026-11-06

### Added

- `Choice`, `#[derive(Choice)]` and `UnknownChoice`: an enum whose variants have no
  fields is a fixed set of words. The derive writes `Choice`, `FromStr` (exact,
  case-sensitive) and `Display` from the variants: each is spelled as its ident in
  kebab-case (`PlayCount` → `play-count`) or its `#[choice(name = "…")]`, and the first
  paragraph of its doc comment is its completion description. A word that is none of them
  fails with `UnknownChoice`, which lists the names and suggests a close one:
  ``expected `bpm`, `title` or `by-artist`; did you mean `bpm`?`` (#8).
- `complete_choice::<C>(typed)`: the names of a choice that start with `typed`, with
  their descriptions — a completer for a string-level parser, completed before it is
  parsed (`.complete(|w: &String| complete_choice::<C>(w)).parse(|w: String| w.parse::<C>())`).
- Spelled `#[derive(Arg)]`: `#[arg(positional)]`, `#[arg(positional = "KEY")]`,
  `#[arg(long)]`, `#[arg(long = "sort", short = 's', metavar = "FIELD")]` spell the
  argument on the type itself, with no `#[derive(Bpaf)]` and no direct `bpaf`
  dependency. The type is a newtype, a struct with one named field, or a fieldless
  enum that derives `Choice`; `Option<T>` is optional, `Vec<T>` repeats, `bool` with a
  name is a switch. The help row is the type's doc comment's first paragraph. A
  `Choice` inside completes its names with no `complete` key;
  `complete = f` takes `fn(&str) -> Vec<Candidate>` and is handed the raw word (so it
  fires on the empty word, even for a `u16`); `files`/`dirs` work as before, and a
  `PathBuf`/`OsString` value still accepts a non-UTF-8 word. The derive also writes
  the snake_case constructor `#[derive(Bpaf)]` would have, so
  `#[bpaf(external(sort_opt))]` still finds it. Types without these keys are derived
  exactly as before (#8, ADR-016).
- A `Choice` enum can be the argument itself: `#[derive(Choice, Arg)]` with
  `#[arg(positional = "KEY")]` parses, completes and reports its names, so
  `fn sort(by: SortKey)` takes the enum directly (#8).
- `ArgHelp`: an argument type whose help row can be replaced where it is used,
  through `ArgHelp::parser_with_help(help)`. Spelled types implement it; a
  hand-written `Arg` may; a `#[derive(Bpaf, Arg)]` type does not.
- `# Arguments` in a `#[command]`'s doc comment: `` * `by` - Order for this listing ``
  replaces that parameter's help row in this command only (its type must be
  `ArgHelp`). An entry may continue on indented lines. The section is not part of
  the details. Naming a parameter that is not parsed (the context, a curried value),
  naming one twice, an empty section, and the section on a `#[group]` or a
  `#[command(default)]` are compile errors (#23, ADR-016).

### Changed

- `cli!` splits words where the source has whitespace, as a shell does, and nowhere
  else: tokens that touch are one word. `cli! { playback set-output --dry-run }` is
  `["playback", "set-output", "--dry-run"]`, and `--since=-7`, `--name="My Event"`,
  `email,phone`, `a/b/c` and `./ev` need no quotes. Quoted words still work. `cli!` is
  now a proc macro in `hicli-macros`, re-exported as `hicli::cli` (#22, ADR-017).
  Edge cases that read differently:
  - `-- foo` is `["--", "foo"]` (was `["--foo"]`);
  - `--` is `["--"]` (was a compile error);
  - `a - b` is `["a", "-", "b"]` (was `["a", "-b"]`);
  - `- 5` is `["-", "5"]` (was `["-5"]`);
  - `"a""b"` is `["ab"]` (was `["a", "b"]`);
  - a number literal is kept as written: `0x10`, `1.50` and `5u8` stay `0x10`, `1.50`
    and `5u8` (were `16`, `1.5` and `5`).

  `#[macro_use] extern crate hicli;` still imports `cli!`. `hicli::testing::to_arg` is
  no longer used by `cli!` and is now `#[doc(hidden)]`.
- The "not a hicli argument type" error (a primitive or a bare `Choice` as a command
  parameter) now suggests the spelled form first,
  ``/// An event id` above `#[derive(Arg)] #[arg(positional = "ID")] struct Id(String);``,
  and names `#[derive(Choice, Arg)]` and `#[derive(Bpaf, Arg)]`. Re-bless trybuild
  `.stderr` files that record it.
- Docs: the README, `examples/playr.rs` and its Quick start spell every one-value
  argument type with `#[arg(…)]`; `examples/demo.rs` stays in bpaf mode, so both forms
  remain documented. A direct `bpaf` dependency is now needed only by a crate that
  uses `#[derive(Bpaf)]`.

### Deprecated

- `UnknownShell` is now an alias of `UnknownChoice`; use `UnknownChoice`.

### Removed

- The hidden `__cli_munch!` macro, which `cli!` used to expand through.

### Breaking

- `Shell` is a `#[derive(Choice)]`. Its `FromStr::Err` is `UnknownChoice` (the
  deprecated `UnknownShell` alias still names it), and its error text changed from
  ``unknown shell `tcsh`: expected one of `bash`, `zsh`, `fish` `` to
  ``expected `bash`, `zsh` or `fish`; did you mean `bash`?``: the typed word is no
  longer repeated, because bpaf already prints ``couldn't parse `tcsh`: `` in front of
  it. `UnknownShell` was a tuple struct; code that built or destructured
  `UnknownShell(…)` no longer compiles.
- **A help is the doc comment's first paragraph, everywhere** (ADR-016, replacing
  ADR-001/ADR-008's "first line"). For a `#[command]` or `#[group]`, consecutive
  lines up to the first blank line are joined with a space into the help; the
  details start after that blank line. A doc comment whose second line directly
  followed the first used to show that line as the start of the details; now it is
  part of the help. **Fix: put a blank `///` line between the help and the
  details.** A `#[command(default)]`'s doc comment is one paragraph (was one line).
  A `Choice` variant's description is likewise its first paragraph.
- `#[arg(…)]` on a type whose parser `#[derive(Bpaf)]` writes now rejects
  `complete = f, complete = g` with ``"`complete` is given twice"`` (was the
  unknown-key message), and the unknown-key message lists the new keys.
  `#[arg(metavar = …)]` without `positional`, `long` or `short` is rejected.
- A `# Arguments` or `Arguments:` line in a command's details (before any examples
  heading) now starts the arguments section instead of being prose, and its
  entries must name the command's parameters. A rustdoc-style `# Arguments` section
  you already had is now read: an entry for a `#[derive(Bpaf)]` type is E0277, and
  any other line is rejected. **Fix: remove the section, or spell the types with
  `#[arg(…)]` and keep the entries as help overrides.**

### Breaking (tests only)

- `cli!` rejects a bracketed group (`(a)`, `[a]`, `{a}`) and a byte or C string
  (`b"a"`, `c"a"`) with a compile error; quote the word as a `"string literal"`
  instead. Before, a bracketed group was stringified (`[a]` was `"[a]"`, `(a b)` was
  `"(a b)"`), and a byte string failed with a type error about `Display`.
- The `cli!` edge cases listed under Changed read differently.

## [0.4.0] — 2026-11-05

No breaking API changes. Output changed: parse errors and command failures start
with `Error: `, help and errors are coloured on a terminal, and long usage lines wrap
between items. Re-bless transcripts of `run` output; `App::parse`-based snapshots are
unchanged.

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

### Fixed

- A usage line longer than 100 columns wraps between items, and every continuation
  is indented under the first item, instead of `bpaf` breaking it wherever column 100
  falls — which could split a `[` or `]` from its option and started the next line
  at column zero (#30, ADR-013 §4). Before:

  ```
  Usage: mdma rekordbox export [--playlist=PLAYLIST]... [--output=OUTPUT] [--format=FORMAT] [--replace
  ]
  ```

  After:

  ```
  Usage: mdma rekordbox export [--playlist=PLAYLIST]... [--output=OUTPUT] [--format=FORMAT]
                               [--replace]
  ```

  A usage line that fits in 100 columns is unchanged, byte for byte. This is a
  workaround for a `bpaf` rendering bug and goes away when `bpaf` fixes it.

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

[0.6.0]: https://github.com/johlrogge/hicli/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/johlrogge/hicli/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/johlrogge/hicli/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/johlrogge/hicli/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/johlrogge/hicli/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/johlrogge/hicli/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/johlrogge/hicli/releases/tag/v0.1.0
