# ADR-007: hicli, and `#[command]`

## Status

Proposed (2026-09-23)

Renames the crate and the vocabulary. **No behaviour changes**: every mechanism
in ADR-001…006 survives exactly as decided, under new names. ADR-001…006 keep
their text as written; this ADR is the dictionary between them and the code.

## Decision

The crate is renamed `verbandi` → **`hicli`** (`verbandi-macros` →
`hicli-macros`), and the vocabulary that was named after the old crate is
renamed with it.

| Old | New | Kind |
|---|---|---|
| `verbandi` / `verbandi-macros` | `hicli` / `hicli-macros` | crates |
| `__verbandi_<ident>` | `__hicli_<ident>` | handle convention (public API, ADR-005) |
| `#[verb]`, `#[verb(name = …)]` | `#[command]`, `#[command(name = …)]` | attribute |
| `Verb<Ctx>` | `Invocation<Ctx>` | the parsed command, arguments bound |
| `VerbResult<T = ()>` | `Outcome<T = ()>` | return-type alias |
| `ExternalVerb` | `ExternalCommand` | external-subcommand manifest |
| `verb_test!` | `cli_test!` | one `#[test]` per typed command line |
| `src/verb.rs` | `src/invocation.rs` | file |
| `VerbArgs` / `VerbFn` / `VERB_NEEDS_DOC` | `CommandArgs` / `CommandFn` / `COMMAND_NEEDS_DOC` | proc-macro internals |
| "a verb" (prose) | "a command" | rustdoc, README, PLAN, ROADMAP, VISION |

Unchanged: `App`, `app!`, `commands!`, `cli!`, `group_mod!`, `#[group]`,
`hicli::group`, `Command<Ctx>`, `hicli::command`, `IntoCommand`, `Arg`,
`#[derive(Arg)]`, `#[arg(complete = …)]`, `Candidate`, `complete_with` /
`complete_files` / `complete_dirs`, `Success`, `Failure`, `IntoOutcome`,
`exit_code_for`, `ArgSchema`, `ArgSpec`.

## Why

**Why rename the crate at all.** `verbandi` is opaque: it neither says what the
crate does nor survives being said out loud. `hicli` does both — *hi cli*, and
*hi-fi → hi-cli*: a high-fidelity command-line interface rendered from your
code. It matches the vision line, **CLI-enable your code**, which the old name
could not. The crate is unpublished, so this is the one free moment; after
0.1.0 a rename costs every consumer a `Cargo.toml` edit and leaves a dead name
on crates.io forever.

**Why `#[command]` and not `#[verb]`.** "Verb" was never a claim about CLIs — it
was a claim about *verbandi*, whose name contains it. The word a reader arrives
with is `command`: it is what clap spells, what `--help` prints, what the tree
node is already called (`Command<Ctx>`), and what the free function
`hicli::command` already is. `#[command]` costs a newcomer nothing to learn, and
it makes the attribute and the primitive it expands to share a name — which is
the crate's own house style, already established by `#[group]` / `hicli::group`.

Four things now share the root word `command`, in three namespaces:
`Command<Ctx>` (type), `hicli::command` (value), `#[command]` (macro), and the
distinct ident `commands!`. They coexist by the same rule `#[group]` and
`hicli::group` already rely on, and **one `use hicli::command;` imports both the
attribute and the function** — which is the ergonomic win, not an accident to be
apologised for. The crate docs state it explicitly.

**Why `Invocation<Ctx>` and not `Verb<Ctx>`.** `Command<Ctx>` is the tree node;
the thing a parse *produces* is one use of it, with arguments bound and only the
context outstanding. "Invocation" names exactly that event, is unambiguous in
this crate, and reads in every position the type appears —
`invocation.path()`, `invocation.run(&ctx)`, and
`App::parse -> Result<(G, Invocation<Ctx>), ParseFailure>` as "the globals and
the invocation".

**Why `Outcome<T = ()>` and not `CommandResult`.** Two reasons.

First, it closes a gap that already exists. `IntoOutcome::into_outcome()`
returns `Result<Success, Failure>`, and the alias for that shape was called
`VerbResult` — the trait and the alias shared no root, so the relationship was
invisible in the rustdoc index. Now `into_outcome() -> Outcome<Success>`, and
the module `outcome`, the alias `Outcome`, the trait `IntoOutcome` and the
four-row table in its docs are one family.

Second, `CommandResult` has a real misreading: `Command<Ctx>` is a type in this
crate, so `CommandResult` parses as `Result<Command, _>` — two rows apart in the
same index.

`hicli::Result<T>` was rejected outright. It is the shadowing trap, and it is
worse here than in `std::io` or `anyhow`: those are used module-qualified
(`io::Result<T>`) or through a deliberate prelude, whereas hicli's convention is
bare single-name imports (`use hicli::{command, group, Arg, Outcome}`). A bare
`Result` import would shadow `std::result::Result` in that module, break
two-parameter `Result<T, E>` unqualified, and make every type mismatch in the
file print an ambiguous `Result<…>`. It remains available as
`hicli::outcome::Result` later if it is ever wanted; adding it is non-breaking,
so declining it now costs nothing.

**Why `cli_test!` and not `command_test!`.** The macro drives `App::parse` and
hands its closure `Result<(G, Invocation<Ctx>), ParseFailure>`. It does not run
a command's body — the body is tested by *calling the function*, which is the
model's central claim. `command_test!` would promise the opposite and quietly
suggest the central claim is unnecessary. `cli!` builds one argv; `cli_test!`
writes one `#[test]` per argv; what the reader of the test file sees is a list
of command lines. The name is theirs, not the implementation's.

**Why prose drops "verb" entirely.** With `#[command]`, `Command<Ctx>`,
`commands!` and `hicli::command` all present, an informal "verb" is a fifth word
for one concept that a reader must learn before the other four make sense.
VISION.md's thesis — *a subcommand is a function, a group of subcommands is a
module, an argument is a type* — never needed it. "Verb" was load-bearing only
while the crate was named after it.

## Alternatives considered

- **Rename the crate, keep `#[verb]`.** Rejected: it strands the word. `verb`
  was justified by `verbandi`; without it, a newcomer must learn a synonym for
  `command` before writing one.
- **Keep "verb" as the informal noun in prose while the attribute is
  `#[command]`.** Rejected: two words for one concept is the exact cost the
  rename exists to remove, and consistency was stated as mattering more than
  either choice.
- **`Call<Ctx>` for the parsed command.** Close second. `call.run(&ctx)` reads
  redundantly, and `Call<Ctx>` suggests "a callable" — which is precisely what it
  is not, since everything but the context is already bound.
- **`Bound<Ctx>`, `Parsed<Ctx>`, `Selected<Ctx>`.** `Bound` collides with
  `std::ops::Bound` in a reader's head; `Parsed<Ctx>` and `Selected<Ctx>` read as
  promises about the `Ctx`, which is the one thing not yet supplied.
- **`CommandResult` for the return alias.** Defensible and maximally
  telegraphic, but misreads against `Command<Ctx>` and leaves `IntoOutcome`
  unrelated to its own alias.
- **Rewriting ADR-001…006 in the new vocabulary.** Rejected: an ADR records what
  was decided *and when*. Rewriting one makes the record lie about its own date.
  Each gets one line in its Status pointing here instead.
- **Renaming `ArgSchema` / `ArgSpec` in the same pass.** Deferred; see
  Consequences.

## Consequences

- **Breaking against the in-repo 0.1.0 in every possible way, and against zero
  external consumers.** Nothing is published. This is the last moment any of it
  is free.
- **`RELEASING.md`'s name check now covers `hicli` *and* `hicli-macros`.** A
  taken macro-crate name blocks the release as hard as a taken runtime-crate
  name; `cargo publish --dry-run --workspace` is the gate.
- **The handle convention is public API and it moved.** Any hand-written group
  handle — the documented escape hatch for a group containing an external
  subcommand — is now `__hicli_<ident>`. Documented in the same places the old
  name was.
- **Every `tests/ui/*.stderr` was re-blessed.** Messages that named `#[verb]`
  or `verbandi` changed text; the rest moved with their file. The blessed diff
  was read as the user who hits each error, not accepted wholesale.
- **`macros/src/commands.rs` no longer spells the handle prefix itself.** It
  re-implemented the `__verbandi_` prefix instead of calling
  `shared::handle_ident`, a second spelling of a rule `macros/src/shared.rs`
  exists to hold once (ADR-005 §7). The rename forced the question and collapsed
  it: the prefix is now defined in exactly one place.
- **`ArgSchema` / `ArgSpec` now read as family with the `Arg` trait, and are
  not.** They describe an *external binary's* arguments for completion
  enrichment; `Arg` is the trait a consumer's own argument types implement. A
  deliberate, recorded near-collision: renaming them is a decision on a different
  axis (ADR-006's vocabulary, not this one's) and folding it in would have made
  this rename non-mechanical. Revisit when `ArgSchema` grows past its v1 shape.
- **`METADEV_PROJECT`, the GitHub repository name and the local working
  directory are outside this ADR.** They are environment, not API; each is a
  separate, reversible move, deliberately not coupled to this commit.
