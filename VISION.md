# Vision — hicli

> The north star, not a spec. The *what* and *how* live in `README.md`, `ROADMAP.md`,
> `docs/PLAN.md` and the ADRs. Refined 2026-09-23 against what has actually been built.

## In one line

**CLI-enable your code.**

Write ordinary, well-typed, well-documented Rust functions. Get a command-line interface
for them — help, completion, exit codes, tests — without writing a CLI.

## The name

**hicli**, as in hi-fi: a *high-fidelity* CLI, rendered faithfully from your code — the
function untouched, the doc comment as the help, the type as the argument. Read it as a
greeting too: *hi cli — adds the line to your command.* (Swedish readers may hear
"hej cli", after the "Hej matematik" school books and the old "Hej XML — jag skyfflar
text" joke. That is intentional.)

## The belief

A command-line tool is code first and interface second. Most CLI frameworks invert
that: you describe the interface in their vocabulary (a derive enum, a builder, a
central dispatch `match`) and hang your code off it. The interface description grows
into the thing you maintain; the code becomes an afterthought that lives somewhere
else; adding a command means editing a shared file everyone else is editing.

It doesn't have to be that way. **A subcommand is a function. A group of subcommands is
a module. An argument is a type.** Those are things Rust already has, already documents,
already type-checks and already tests. The framework's job is to *derive* the interface
from them — and to get out of the way.

## The programming model

This is a programming model, not a parser library. It is opinionated in ways a general
parser cannot be:

- **Code-centric.** The function *is* the command: `#[command]` leaves it exactly as
  written — same name, same signature, callable from a test, callable from another
  function. Nothing is generated that you would ever have to call instead of your own
  code. Registration is by containment: a function in a `#[group]` module is a command
  in that group, in source order. No list to keep in step, no dispatcher to grow.

- **Statically typed, newtypes everywhere.** An argument is a type you name: `Id`,
  `EventDir`, `Moniker` — never a bare `String` or `PathBuf`. The type carries the
  metavar, the help, the validation and the completion, once, and every command that
  takes it inherits all four. A primitive in a command's signature is a compile error
  that shows you the newtype to write. The framework ships no argument types of its
  own, on purpose: the vocabulary of your CLI is yours.

- **Documentation is the interface.** Your doc comment *is* the help text — on a
  command, on a group, on an argument type. Help cannot go stale, cannot be forgotten
  (a missing doc comment is a compile error), and reads the same in `--help`,
  in `cargo doc` and in the source.

- **Function-oriented, and willing to pay for it.** Where CLI expressiveness and a
  natural programming experience conflict, the programming experience wins. Arguments
  are plain parameters composed in declaration order; effects come back as values
  (`Success`, `Failure`), never as `process::exit`. If a parsing feature would cost the
  function its ordinary shape, we leave the feature out.

- **Unit-testable by construction.** Every layer has an in-process seam: an argument
  type's parser, the command function itself, the parsed tree (`cli_test!`, written
  like a terminal session), the whole run with its exit code, and shell completion —
  no subprocess, no string-scraping. The next step on this road is making the *bodies*
  as testable as the parsing: typed alternatives to raw `File`, `Path` and directory
  access so a command can be exercised against an in-memory world.

## What we're really buying

Not fewer lines — **locality and honesty.** Everything about a command is in one place,
in the language you already write, checked by the compiler you already run. On a large
CLI that difference compounds: adding a command is adding a function in the right
module, and the tool's help, completion and tests follow from that automatically.

## The bets that stay

1. **A CLI can span several binaries and still feel like one** — the `git` model, with
   `--help` and completion describing external subcommands without executing them.
2. **Good error ergonomics are not optional.** Typo suggestions on par with `clap`;
   compile errors that teach the model rather than merely reject.
3. **Discovery is trust-conscious by construction.** No implicit `$PATH` scanning;
   externals are allowlisted by absolute path and never handed secrets implicitly.
4. **Built on `bpaf`** (parsers as plain values), which is what makes in-process
   testing and composition honest rather than aspirational. `bpaf` is re-exported,
   never wrapped; argument spelling stays `bpaf`'s, so there is one dialect to learn.

## What success looks like

- **It proves itself on real CLIs.** `moniker` migrates first, with identical
  behaviour, exit codes and passing journey tests; then `mdma-cli` and `ctx`.
- **The modular path is genuinely easier** than the central-match path it replaces. If
  writing a new command ever feels heavier than a `match` arm, the design has failed.
- **A reader can learn the model from one example** and predict the rest: function,
  module, type, doc comment.
- **It's a good crates.io citizen** — documented, dual-licensed, semver-disciplined,
  small enough to read in one sitting.

## Non-goals

- **Not a `clap` replacement for everyone.** It is for people who want their CLI to be
  a thin, typed, documented skin over code they would write anyway.
- **Not maximal CLI expressiveness.** Exotic parsing shapes that would distort the
  function are out of scope; `bpaf`'s primitives remain reachable by hand for the rare
  case.
- **Not a kitchen sink.** Every mechanism is earned by a real consumer's pain and
  recorded in an ADR. Auto-registration by link-time magic is rejected outright.

## Guiding principle

Subtractive design. The win is a *smaller, clearer* way to build a CLI. Prefer
sharpening the three concepts — function, module, type — and deleting scaffolding
over adding knobs. When in doubt, leave it out until a consumer proves it's needed.
