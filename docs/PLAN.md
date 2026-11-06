# `hicli` — CLI-enable your code: a modular, publishable subcommand framework

> This is the approved design/plan for `hicli`. It doubles as the working reference
> while the crate is built. Status: scaffolding + v1 skeleton.

## Context

**Why this exists.** The hypothesis: *pretty much any clap CLI ends up less modular than it
could be.* Ideally one subcommand = one function/type with annotated arguments, its parser and
dispatch **derived by composition**, not hand-written into a central `match`. A scan of the
author's projects confirms the gap — every clap CLI uses derive enums + a hand-written dispatch,
from clean (`cargo-polylith`) to monolithic (`mdma-cli`, 7292 lines in one file; moniker's
~330-line `dispatch()` match; `ctx`).

**Decisions:**
- **Deliverable:** a **standalone, crates.io-publishable framework crate `hicli`**, its own
  repo, a metadev project with its own devenv.
- **Consumers:** the author's CLIs adopt it as an external dependency — **moniker first** (its
  handlers are already parser-free), then `mdma-cli` (biggest payoff) and `ctx`. Consumer
  migrations are follow-up work.
- **Mechanism:** built on **bpaf** (parser-as-value combinator style); `hicli` re-exports
  `bpaf` so it states one version for consumers (see "Consumer footprint of the macros" below
  for why this is no longer a single shared dependency edge). ~~No bespoke proc-macro for v1.~~
  Superseded by [ADR-004](adr/004-function-first-verbs.md): a small proc-macro crate,
  `hicli-macros`, ships exact-pinned behind `hicli::command`, once a real
  consumer's (moniker's) pain justified it. See "The framework API" below.
- **Must not regress from clap:** first-class **testability**, **git-style multi-binary
  subcommands with shared help + completion**, and **typo/"did you mean?" suggestions**.

## bpaf capability check (grounds the design)

- **Subcommands are values:** `OptionParser<T>::command(name)` yields a `Parser<T>` to store/compose.
- **Command owns its parser next to its handler:** args via `#[derive(Bpaf)]` or `construct!`.
- **"Run this command" polymorphism is a `.map()`:** `parser.map(move |args| Invocation::new(path, move |ctx| run(args, ctx).into_outcome()))`, combined via `construct!([a,b,c])` (built up node by node in [`Command::mount`](../src/command.rs)). No central match, and no trait object — `Invocation<Ctx>` is a struct.

**Three constraints `hicli` handles for consumers:**
1. **Exit-code remap.** bpaf: parse error → 1, help/version → 0. Consumers often want usage → 2.
   `hicli` calls `run_inner`, intercepts `ParseFailure`, and **remaps** (never
   `OptionParser::run()`, which auto-exits with bpaf's codes). Codes are `u8`
   throughout (matching `std::process::ExitCode` and POSIX's low-8-bits rule);
   [`App::run`](../src/app.rs) returns `std::process::ExitCode` itself, so a
   consumer's `main` is just `app().run(mk_ctx)` with no `try_from(…).unwrap_or(1)`
   tail. [`App::run_with_args`] and [`exit_code_for`] keep returning `u8`, because
   `ExitCode` is not comparable and a test seam must be assertable.
2. **Secrets-off-argv is a consumer concern.** bpaf's `env()` shows in `--help` and is settable on
   argv. `hicli` keeps context-building in a consumer-supplied `mk_ctx` closure so things like
   `MONIKER_PASSPHRASE` stay hand-read env vars, never fields.
3. **Globals live at the top level, not per subcommand.** `hicli` composes
   `construct!(globals, tree)`, so a global is accepted both before and after the subcommand
   (`app --home x status`, `app status --home x`, even `app event create dir NAME --home x` —
   bpaf's positional parsers stop at a dashed token, so a free-form positional does not swallow
   it). The real delta from clap's `global = true` is structural: no per-subcommand redefinition
   or propagation — globals are parsed once and handed to `mk_ctx`; a subcommand cannot declare,
   shadow or override them. **Externals are the exception:** everything after an external
   subcommand's name is forwarded to the child verbatim, so a global must *precede* the
   external's name (`app --home x foo`, never `app foo --home x`).

## The framework API

Commands are functions (ADR-004, ADR-005): a consumer never implements a trait or
builds a tree with a chain of method calls. `command`/`group`/`external` and
`App` are the primitives; `#[command]`, `#[group]`, `commands!` and
`app!` are macros that expand to them, documented in the next section.

```rust
pub use bpaf; // consumers state ONE bpaf version, cargo unifies it (see below)

/// A command finished; the `u8` is the process exit code, usually `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Success(u8);
impl Success {
    pub const OK: Success;                 // code 0
    pub fn code(n: u8) -> Success;          // Success::code(0) == Success::OK, nothing normalised
    pub fn get(self) -> u8;
}

/// A command failed; the variant picks the code, never a caller-chosen number.
#[non_exhaustive]
pub enum Failure { Usage(String), Runtime(String) }   // code(): 2 / 1
impl Failure {
    pub fn usage(m: impl Into<String>) -> Self;
    pub fn runtime<E: Display>(e: E) -> Self;
    pub fn code(&self) -> u8;
    pub fn message(&self) -> &str;                     // always present, never `Option`
}
pub type Outcome<T = ()> = Result<T, Failure>;

/// Everything a command may return, normalised to `Outcome<Success>`.
/// Four shapes: `()`, `Success`, `Result<(), E>`, `Result<Success, E>` with
/// `E: Into<Failure>` (ADR-012). The success side is closed.
pub trait IntoOutcome { fn into_outcome(self) -> Result<Success, Failure>; }
/// Any error is a `Failure::Runtime` whose message is its `source()` chain.
/// `Failure` never implements `Error` — the blanket depends on it.
impl<E: Into<Box<dyn Error + 'static>>> From<E> for Failure { /* … */ }

/// A parsed subcommand: arguments already bound, waiting only for the context.
/// Not something a consumer builds — it is what a `Command`'s parser produces.
pub struct Invocation<Ctx> { /* path + a bound FnOnce(&Ctx) -> Result<Success, Failure> */ }
impl<Ctx> Invocation<Ctx> {
    pub fn name(&self) -> &'static str;       // the last path segment
    pub fn path(&self) -> &[&'static str];    // e.g. ["event", "create"]
    pub fn run(self, ctx: &Ctx) -> Result<Success, Failure>;
}

/// One node of the command tree — a leaf, a group, a curried group, a group's
/// default, or an external. `O` is what parsing the node yields. A leaf and a
/// group are the same type, which is what makes a parent's children a plain
/// `Vec<Node<O>>` and `commands![…]` homogeneous.
pub struct Node<O> { /* name, help, details, footer, aliases, listing, leaf|default|group|curried */ }
/// What an `App` mounts: a node yielding an `Invocation`.
pub type Command<Ctx> = Node<Invocation<Ctx>>;
impl<O> Node<O> {
    pub fn details(self, details: impl Into<String>) -> Self; // bpaf header: reflowed prose
    pub fn footer(self, footer: impl Into<String>) -> Self;   // after the options, verbatim (ADR-008)
    pub fn alias(self, alias: &'static str) -> Self;          // repeatable; unlisted; path stays canonical
    pub fn hide(self) -> Self;                                // out of help/completion/suggestions; parses
    pub fn name(&self) -> &'static str;
    pub fn by_default(self) -> Self;  // `#[command(default)]`: runs when no sibling is named (ADR-011)
}
// `help` is mandatory (ADR-001): the parent's one-line command entry AND the
// heading of `<cmd> --help`.
pub fn command<Ctx, A, R>(name: &'static str, help: impl Into<String>,
                          parser: impl Parser<A> + 'static,
                          run: impl Fn(A, &Ctx) -> R + Clone + 'static) -> Command<Ctx>
    where A: 'static, R: IntoOutcome, Ctx: 'static;
pub fn group<O: 'static>(name: &'static str, help: impl Into<String>,
                         children: Vec<Node<O>>) -> Node<O>;
pub fn external<Ctx: 'static>(ext: ExternalCommand) -> Command<Ctx>;   // git-style plugin

// Curried commands and default commands (ADR-011). `Node<O>` is the one node type;
// `Command<Ctx> = Node<Invocation<Ctx>>`, `Curried<T, Ctx> = Node<Partial<T, Invocation<Ctx>>>`,
// and `group` is generic over `O`. Source-compatible with 0.1.0, except that a turbofish
// `group::<Ctx>(…)` must be dropped or written `group::<Invocation<Ctx>>(…)`.
pub fn curried_command<T, Ctx, A, R>(name: &'static str, help: impl Into<String>,
                                     parser: impl Parser<A> + 'static,
                                     run: impl Fn(A, T, &Ctx) -> R + Clone + 'static)
    -> Curried<T, Ctx>;                    // parses A; yields a function awaiting T
pub fn group_args<T: 'static, O: 'static, P: Parser<T> + 'static>(
    name: &'static str, help: impl Into<String>, arg: impl Fn() -> P + 'static,
    fixed: Vec<Node<O>>, bound: Vec<Node<Partial<T, O>>>) -> Node<O>;
                                           // `#[group(args = T)]`: parse a T after the name, hand it on

/// Anything that denotes a `Command`: a `Command` itself, or a zero-argument
/// callable returning one — a `#[command]`/`#[group]` handle. What
/// lets `commands![create, event]` mix handles without the call site writing `()`.
pub trait IntoCommand<Ctx> { fn into_command(self) -> Command<Ctx>; }

/// An argument is a type (ADR-006): one required method, no metavar, no
/// attributes, nothing about spelling — that lives in `#[derive(Bpaf)]` on the
/// same type. `#[command]` calls this, never a bpaf constructor by name.
pub trait Arg: Sized + 'static { fn parser() -> impl bpaf::Parser<Self>; }
// hicli ships no impls of `Arg` — not for `String`, `u64`, `PathBuf`, … — on
// purpose (see "The framework API" below and the Risks section).

/// One completion candidate: the value to insert, and an optional one-line
/// description. A plain alias, not a struct, so no conversion and no turbofish.
pub type Candidate = (String, Option<String>);

// Attach a dynamic completer, or hand completion to the shell itself:
pub fn complete_with<T>(parser: impl bpaf::Parser<T>,
                        candidates: impl Fn(&T) -> Vec<Candidate>) -> impl bpaf::Parser<T>;
pub fn complete_files<T>(parser: impl bpaf::Parser<T>) -> impl bpaf::Parser<T>;
pub fn complete_dirs<T>(parser: impl bpaf::Parser<T>) -> impl bpaf::Parser<T>;

/// Composition builder.
pub struct App<G, Ctx> {
    /* global parser + the top-level commands, kept as data until folded */
}
impl<G: 'static, Ctx: 'static> App<G, Ctx> {
    pub fn new(globals: impl Parser<G> + 'static) -> Self;   // a parser, not an OptionParser:
                                                             // globals must compose *as a parser*
                                                             // with the command at the top level
    pub fn command(self, c: impl IntoCommand<Ctx>) -> Self;
    pub fn commands(self, cs: impl IntoIterator<Item = Command<Ctx>>) -> Self;
    pub fn about(self, about: impl Into<String>) -> Self;     // above `Usage:` (ADR-008)
    pub fn version(self, v: impl Into<String>) -> Self;       // -V/--version, exit 0; both reserved
    pub fn run(self, mk_ctx: impl FnOnce(G) -> Result<Ctx, Failure>) -> std::process::ExitCode;
                                     // = ExitCode::from(run_with_args(env::args().skip(1), …))

    // --- test seams (in-process, no subprocess) ---
    pub fn into_parser(self) -> bpaf::OptionParser<(G, Invocation<Ctx>)>;
    pub fn parse<I, S>(self, args: I) -> Result<(G, Invocation<Ctx>), bpaf::ParseFailure>
    where I: IntoIterator<Item = S>, S: Into<String>;   // takes `cli! { … }` by value
    pub fn run_with_args<I, S>(self, args: I, mk_ctx: impl FnOnce(G) -> Result<Ctx, Failure>) -> u8
    where I: IntoIterator<Item = S>, S: Into<String>;   // the whole run path, argv supplied
    pub fn complete<I, S>(self, args: I) -> Vec<String>
    where I: IntoIterator<Item = S>, S: Into<String>;   // the completion-level test seam (ADR-006)
}
pub fn exit_code_for(f: &bpaf::ParseFailure) -> u8;    // Stderr => 2 (usage), else 0

/// An installable completion script (ADR-009). `#[non_exhaustive]`; parses from
/// and displays as its lowercase name. hicli ships no `Arg` for it (ADR-006).
#[non_exhaustive]
pub enum Shell { Bash, Zsh, Fish }
impl FromStr for Shell { type Err = UnknownShell; /* "bash" | "zsh" | "fish" */ }
pub struct UnknownShell(/* the name it did not know */);   // Display + Error
pub fn completion_script(shell: Shell, bin: &str) -> String;
                                     // bpaf 0.9.27's templates, revs 8/7/9; panics unless
                                     // `bin` is non-empty [A-Za-z0-9._+-] (pasted unquoted)
```

- **Tree by composition:** a group *is* a `Command<Ctx>` with children — not a
  second type — so children fold via `construct!([acc, next])` one command at a
  time ([`Command::mount`](../src/command.rs)); a leaf's `bpaf` parser is `.map`ped
  to an `Invocation`. Its `path()` is not stored on the `Command` — it is assigned
  during this fold, from the names on the way down, which is what lets `event
  create` and `id create` disambiguate in a test without running either.
- **Globals + context:** top-level parser yields `(G, Invocation<Ctx>)`; `App::run` hands
  `G` to `mk_ctx`, then `invocation.run(&ctx)`. Maps `ParseFailure` → 2/0 and the
  command's outcome → `Success`'s code (silent) or `Failure`'s code (2 usage / 1 runtime,
  message to stderr). `hicli` never calls `std::process::exit`: every path
  returns a code (or, from `App::run` itself, an `ExitCode`) to the consumer's
  `main`.
- **An argument is a type (ADR-006):** `#[command]` never synthesises a call by
  name — it calls `<T as Arg>::parser()` for each owned parameter. `Arg` is
  hand-implementable (the same relationship `#[command]` has to `hicli::command`)
  or derived with `#[derive(Arg)]` beside `#[derive(Bpaf)]`, which writes one
  impl calling the constructor `#[derive(Bpaf)]` already generated, spanned at
  the type's ident so a missing `#[derive(Bpaf)]` reports at the type. An
  optional `#[arg(complete = …)]` — a function, or the shorthands `files`/`dirs`
  — attaches completion to the *type*, so every command that takes it inherits the
  completion for free; it is a compile error on a multi-field struct or an enum,
  because a whole-value completer cannot tell which field is mid-word.

**Consumer-side subcommand (the pattern this enables) — see ADR-005 for the full
contract:**
```rust
/// create an event
///
/// The event directory is created if it does not exist.
#[hicli::command]
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
    /* call domain handler + print */
    Ok(())
}
```
expands to the function **verbatim** — same name, signature, visibility, body —
plus one hidden sibling in the value namespace, the handle:
```rust
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome { /* the body, verbatim */ }

#[doc(hidden)]
#[allow(unreachable_pub)]
pub fn __hicli_create() -> ::hicli::Command<Ctx> {
    let a0 = <EventDir as ::hicli::Arg>::parser();   // one Arg::parser() call
    let a1 = <EventName as ::hicli::Arg>::parser();  // per owned param, bound to a
                                                          // local, in declaration order
    ::hicli::command(
        "create", "create an event",
        ::hicli::bpaf::construct!(a0, a1,),
        |(a0, a1), ctx: &Ctx| create(a0, a1, ctx),
    ).details("The event directory is created if it does not exist.")
}
```
There is no `mod create { run }`: `create` is the function, callable by its own
name from its neighbours, its tests, and itself. `EventDir`/`EventName` are
ordinary `#[derive(Bpaf, Arg)]` structs the consumer writes — `#[command]` never
sees an argument's spelling, only its type:
```rust
/// Where the event lives.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]     // completion travels with the type, inherited by every
pub struct EventDir(#[bpaf(positional("DIR"))] pub String);   // command that takes one
```
(bound locals, not inlined calls — `construct!` accepts only a single-segment
`ident()` call as an element, verified against `bpaf` 0.9.28). A group is an
annotated module, not a list:
```rust
/// events and instances
#[group]
pub mod event {
    /// create an event
    #[command]
    pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome { … }

    /// show an instance's roster
    #[command]
    pub fn roster(ev: EventRef, ctx: &Ctx) -> Outcome { … }
}

fn app() -> App<Globals, Ctx> {
    app! { globals: globals(), commands: [event, apply] }   // = App::new(globals())
}                                                            //   .commands(commands![…])
```
A child is a child because it is in the module — no list to keep in step with
the code. A file module is the same attribute on the declaration (ADR-010):
```rust
/// events and instances
#[group]
pub mod event;
```
which emits `pub mod event;` (with a synthesised `#[path]`) plus the same
sibling handle, discovering children by reading `src/event.rs`. `commands!`
and `app!`'s list are unchanged in shape: **a bare path is always a handle**
(`event` → `__hicli_event()`, `a::b` → `a::__hicli_b()`); anything else is
an expression through `IntoCommand::into_command`.

**Object then verb, and a default (ADR-011).** `#[group(args = SourceName)]` parses one
`SourceName` (its `Arg::parser`, completion included) after the group's name; its
`#[command(curried)]` children curry over their first owned parameter and receive it, while
unmarked children stay fixed siblings, matched first (`source list` beside `source NAME sync`).
`#[group(curried = T)]` is a plain group after an enclosing value, all children curried.
`#[command(default)]` runs when no sibling is named (`search jazz` beside `search
fact-values-for genre`), answers to no word of its own, and has the group's path. Both are
sugar over `curried_command`/`group_args`/`Node::by_default`; the function stays untouched. Known
limits (bpaf 0.9.27) are in ADR-011.

**Consumer footprint of the macros — verified at the ADR-004 spike (2026-09-16)
and the ADR-005 spike (2026-09-17, see each ADR's "Consequences"):**

- **A consumer adds `bpaf = "0.9"` directly**, no features (feature unification
  supplies `derive` from hicli's own dependency). `#[derive(Bpaf)]` expands to
  `::bpaf::…` absolute paths, which resolve only against a crate's own extern
  prelude — `use hicli::bpaf;` does not help. This weakens the "consumers pin
  one `bpaf` version through `hicli`" claim above to: **hicli states the
  required version, cargo unifies it** — not a single re-exported dependency edge.
- **`#[command]` emits `construct!($($ctor()),* ,)` uniformly** for every arity ≥ 1,
  never the bare arity-1 form: `construct!(a())` compiles but yields a
  parser-of-parser with a confusing `Clone` error downstream; the trailing comma
  (`construct!(a(),)`) is what yields the bare `A`. Arity 0 is `pure(())`.
- **bpaf's positionals-last rule leaks into a `#[command]` function's parameter
  order**, and the macro cannot check it: since arguments are composed with
  `construct!` in declaration order, an argument struct containing a positional
  must be declared *after* every struct that doesn't (switches/options first,
  positionals last — see the Risks section and `EventRef`/`ShareOpts` in
  `docs/sketches/moniker.rs`). Getting this wrong is a `bpaf` runtime panic when `--help`
  renders (in any build), not a compile error; hicli renders `--help` once at fold time in debug
  builds so it surfaces on the first debug run or test (#11).
- **`#[group]` requires a doc comment, and a blank one is a compile error too.**
  ADR-001's rule (no doc comment on a command) is enforced by rustc, the same way
  `#[command]` enforces it — including a doc comment consisting only of blank `///`
  lines, which used to compile and then panic at the list-form `group!`'s
  generated handle. A proc macro can read a doc comment's text, not just see that
  the attribute exists, so the whole class of failure moved from runtime to
  compile time (ADR-005).

## Testability (first-class, in-process)

bpaf's edge over clap: parsers are plain values; `run_inner(Args::from(&[...]))` returns
`Result<T, ParseFailure>` with **no process side effects** — `ParseFailure` exposes rendered text
(`unwrap_stderr()`/`unwrap_stdout()`) and `exit_code()`. `hicli` surfaces this via
`App::parse` / `into_parser` / `exit_code_for`.

**Five layers:** L1 an argument struct's own `#[derive(Bpaf)]`-generated parser (assert fields) —
tested **once per shared struct**, not once per command that uses it, which is the whole point of
`EventRef`-style reusable argument groups; L2 the command function itself, called the way it is
declared (`create(dir, name, &ctx)`) — an ordinary function test, not a call through a handle or a
synthetic `run` — because ADR-005 leaves the function exactly as written; L3 `App` dispatch + exit
intent — via `parse`/`into_parser` (selected `invocation.name()`/`invocation.path()`, remapped
exit code, rendered stderr) *and*, at run level, via `run_with_args` (the full pipeline: parse →
remap → `mk_ctx` → `invocation.run` → exit code, including `Success`/`Failure` code propagation).
`run_with_args`
does not capture stdout/stderr, so only the returned `u8` is assertable there; rendered text stays
an L3-`parse` or unit-level assertion; L4 consumer domain handlers (unchanged); L5 completion, via
`App::complete(args) -> Vec<String>` (ADR-006) — in-process, no shell, driven through `bpaf`'s own
completion wire protocol at revision 1, which is pinned deliberately: **revision 2 driven through
`run_inner` terminates the process**, a hole in `bpaf`'s exit-free seam outside hicli's control,
so `App::complete` never asks for it and a consumer's test never touches the protocol revision at
all. Revision 1 cannot express a shell directive, so `complete_files`/`complete_dirs` produce no
values there — those tests assert the parser still parses, not a candidate list.

### Terminal-shaped test macros (the tests read like what the user types)

A signature ergonomic goal: a test should show, verbatim, **the command line the user types**,
mapped to a test function. `hicli` ships two macros:

- **`cli! { … }`** — takes the command line as **raw tokens** (no wrapping string), so it reads
  exactly like typing in a terminal, and expands to the arg vector for `App::parse`:
  ```rust
  let (_g, invocation) = app().parse(cli! { event create x "My Event" }).unwrap();
  assert_eq!(invocation.name(), "create");
  ```
  Words are separated by whitespace, as in a shell: tokens that touch in the source join into one
  word (`set-output`, `--dry-run`, `--since=-7`), and a **string-literal token** is taken by value —
  that's how an argument containing spaces/special chars is written (ADR-017: `cli!` is a proc macro). Tokenizer caveat (documented on the macro):
  a single-quoted multi-word arg (`'My Event'`) does not lex in Rust, so a spaced argument MUST be
  a double-quoted literal `"My Event"`; single-token args need no quotes (`event create x`).
- **`cli_test! { … }`** — generates a `#[test]` per case; each case's left side is the typed
  command-line tokens (same form as `cli!`), parsed via a consumer-provided `app()` builder, and
  the `Result<(G, Invocation<Ctx>), ParseFailure>` is handed to the closure — `path()` disambiguates
  two commands of the same name in different groups. The tokens *are* the test's documentation:
  ```rust
  cli_test! {
      app: crate::app();
      create_ok: { event create x "My Event" } => |r| {
          let (_g, invocation) = r.expect("parses");
          assert_eq!(invocation.name(), "create");
      },
      missing_name_usage: { event create x } => |r| {
          let e = r.unwrap_err();
          assert_eq!(hicli::exit_code_for(&e), 2);
          assert!(e.unwrap_stderr().contains("NAME"));
      },
  }
  ```
  (Exact `app()`-supply convention — a leading `app: <expr>;` header vs per-case — finalized during
  implementation; keep it minimal.) `cli_test!` deliberately covers the parse layer only — a
  run-level arm would need a second header (`mk_ctx`) and a different case grammar, i.e. a second
  macro wearing the same name; deferred until a consumer's run-level suite justifies it.
  `cli!`/`cli_test!` are unaffected by ADR-004's function-first rewrite, including their syntax —
  only the type flowing through the closure changed, from a boxed trait object to `Invocation<Ctx>`.

These macros are the crate's headline testing UX and must be covered by the crate's own tests +
shown in the example CLI.

## Multi-binary subcommands with shared help + completion (the git model)

`app foo ARGS…` exec's the binary registered for `foo` (by absolute path — the git-style
`app-foo` naming is a convention, not the contract), yet `app --help` and completion still
enumerate `foo`.
clap's `allow_external_subcommands` dispatches but the subtree is opaque. `hicli` carries **one
manifest both the parser and help/completion read**:
```rust
pub struct ExternalCommand { name: &'static str, help: String, bin: PathBuf, schema: Option<ArgSchema> }
```
An external becomes a bpaf `command(name)` whose parser collects the remainder verbatim
(`any::<String>(...).many()`) and `.map`s into an `Invocation` that runs
`Command::new(ext.bin).args(rest).status()`, propagating the child's exit code as a value —
`Ok(Success::code(n))`, since the child already reported for itself and a `Success` prints
nothing — never by exiting the process; on Unix a signal death becomes `128 + N`, matching the
shell. Only hicli's own failure to run the binary at all is a `Failure`.

- **Advertising the subtree — static allowlist manifest is the authority**, with an optional,
  opt-in runtime handshake (`app-foo --dump-command-manifest`) only to enrich completion for
  already-allowlisted binaries. Help/completion render **without executing any sibling**;
  discovery is never implicit.
- **Completion across the boundary:** bpaf's dynamic completion completes `app <tab>` → `foo` from
  the manifest with zero child involvement; deeper completion uses `schema` in-process, or
  (handshake on) delegates to `app-foo --bpaf-complete-style-bash`.
- **Trust posture (baked in):** (1) no implicit `$PATH` scan, ever; (2) allowlist by absolute
  path, resolved once at registration; (3) no implicit secret forwarding — external gets argv
  only; (4) optional per-binary hash pin.

## Typo / "did you mean?" suggestions (no regression)

bpaf has built-in suggestions for both misspelled commands (since 0.5.1) and flags (since 0.7.10)
— no categorical regression; exact parity with clap's threshold/phrasing not guaranteed.
1. **Golden matrix** in tests (via `cli!`/`cli_test!` + `unwrap_stderr()`): misspelled
   top-level/nested commands, long flags, short/long confusion — assert stderr names the target.
2. **Gap-closer using the same registry:** on `Err(ParseFailure::Stderr)` with no suggestion, run
   a Levenshtein pass (≤ 2; `strsim`) over the registered **top-level command names**, applied to
   the command position only, and append `` did you mean `<name>`? ``. Nested commands and flags
   are covered by bpaf's own suggestions (see the ADR `suggestion-registry-is-top-level-only`).
   The `(name, help)` pair supplied at registration feeds help and completion, rendered by
   bpaf from the `command(name).help(help)` registrations; the top-level name list feeds the
   suggestion gap-closer, and nothing else reads it.

## v1 scope (subtractive)

**In:** `Success`/`Failure`/`IntoOutcome`, `Invocation<Ctx>` (a struct), `Command<Ctx>`/`IntoCommand`
(`command`/`group`/`external`) + `App` (`command`/`commands`/`run` + test seams, `complete`); the
proc-macro crate `hicli-macros` (exact-pinned): `#[command]` (leaves the function alone, adds a
handle; calls `<T as Arg>::parser()` per owned param rather than a synthesised name), `#[group]`
(an annotated module — inline, or a file module `#[group] pub mod event;` (ADR-010) — discovered
children, no list), `commands!` (a bare path
names a handle); the `app!` declarative macro forwarding into `commands!`; `cli!`/`cli_test!`;
the suggestion gap-closer; **`Arg` + `#[derive(Arg)]` + the `files`/`dirs` completion shorthands
(ADR-006)** — an argument is a type that parses itself, and completion is attached to that type
once and inherited by every command that takes it; **command metadata (ADR-008)** — the
`alias = "…"` (repeatable) and `hide` keys on `#[command]`/`#[group]` (with `Command::alias` /
`Command::hide` beneath them; every spelling is a known exact match for the gap-closer, only listed
canonical names are suggested), the doc comment's **examples footer** (a trailing `# Examples` /
`Examples:` section rendered verbatim after the options via `Command::footer`), and
`App::about` / `App::version` on the composition root; **`completion_script(Shell, bin)`**
(ADR-009) — the shell script a user installs to wire TAB to the app.

**Also in: curried commands, `args` groups and default commands (ADR-011)** —
`#[command(curried)]` (curried over its first owned parameter; the handle returns
`Curried<T, Ctx>`), `#[group(args = T)]` (parses one `T` after the group's name, curried
children receive it, unmarked children are fixed siblings matched first),
`#[group(curried = T)]` (a plain group after an enclosing value) and `#[command(default)]` (runs
when no sibling is named); the runtime primitives `curried_command`, `group_args` and
`Node::by_default` over one generic `Node<O>`.

**Also in: command errors convert through `Into<Failure>` (ADR-012)** — a blanket
`impl<E: Into<Box<dyn Error + 'static>>> From<E> for Failure` makes any std error,
`anyhow::Error`, `eyre::Report`, `Box<dyn Error>`, `String` or `&str` a `Failure::Runtime`
(exit `1`, message `outer: mid: root`), and `IntoOutcome`'s two `Result` impls are generic over
`E: Into<Failure>`, so `fn sync(..) -> color_eyre::Result<()>` is a command as written. No
eyre/anyhow feature or dependency; `Failure` never implements `Error`; no downcasting back to a
code. `Box<dyn Error + Send + Sync>` is not accepted.

**Deferred:** nested `args` groups (they parse at runtime but have no constructor or macro form);
a default in a curried group; a top-level default; a default that also answers to its own name
(recipe: a second `#[command]` calling the same function); a dedicated `source NAME --help` page;
typo suggestions inside curried groups (ADR-002's scope stands) (all ADR-011);
`inventory`/`linkme` auto-registration (still rejected outright — discovery here is lexical and
local, reading one module body or one file, not link-time collection); argument-level proc-macro
sugar beyond `#[arg(complete = …)]` (a field-attribute dialect naming metavar/positional-vs-option
— rejected by ADR-004 in favour of consumer-written `#[derive(Bpaf)]` argument structs, revisit
only if that proves unusable from a crate that doesn't depend on `bpaf` directly); a
field-composing `#[derive(Arg)]` for composite types (ADR-006 — a composite hand-writes
`Arg::parser()`, six lines, composing its parts' own `Arg::parser()`s); `#[arg(parser = …)]` for a
type whose `bpaf` constructor was renamed (a three-line hand-written impl covers it); completion
masks on the shell shorthands (`complete = files("*.toml")`); globals-aware completion (a
completer sees only the parsed value, never `--home` — pre-parsing the globals leniently in
completion mode needs a type-erased process-global stash with a runtime-fallible downcast, which
hicli cannot synthesise); a usage-line override; deprecated-name warnings (an alias that prints
a hint); visible aliases (`list, ls` in the command list); elvish completion — all deferred
until a consumer's pain names them, not built ahead of that evidence.

**Rejected:** a helper (or a documented recipe) for accepting an option value that starts with
`-` as a separate word (`--since -7`) (#29). A dash-led value is written glued, `--since=-7`, and
a plain `argument` reads it; the two-word form stays a usage error whose bpaf message names the
glued form. This keeps the standard rule that a word starting with `-` is an option.

## Verification

- `devenv shell`: build, test (parser/dispatch/suggestion golden matrix + macro tests, all
  in-process), `clippy -D warnings`, `fmt --check`.
- **`#[command]`/`#[group]`/`#[derive(Arg)]`'s rejections, as `trybuild` UI tests:** no
  doc comment, two reference parameters, a `&mut` parameter, a `self` receiver, generics/lifetimes,
  an empty group, a module-relative context type, `#[group]` on a non-module, a `#[group]` file
  module whose file is missing, a `#[group] mod b;` nested inside an inline `#[group]` module, a command
  parameter whose type does not implement `Arg`, `#[derive(Arg)]` without `#[derive(Bpaf)]`,
  `#[arg(complete = …)]` on a multi-field struct or an enum, `#[derive(Arg)]` on a generic type,
  and ADR-008's attribute keys and examples slot — an unknown key, `name`/`hide` given twice, an
  empty name or alias, one with whitespace or a leading `-`, a repeated alias, an alias equal to the
  command's or the group's own name, an examples heading with nothing under it —
  fixed `.rs`/`.stderr` pairs under `tests/ui/` (the trybuild suite), so a rejection's wording is
  checked like any other behaviour, not just that it fails to compile.
- **Group discovery and file resolution, as their own golden matrix:** `#[group]` on an inline
  module discovers `#[command]`/`#[group]` children in source order, honours a child's `#[cfg]`, and
  lifts a context type from the first `&T` — `tests/groups.rs` (16 cases). `#[group]` on a
  file-module declaration resolves it to a file against the Reference's mod-rs/non-mod-rs rules,
  reads it, and nests groups across file boundaries — `tests/group_files.rs` plus
  `tests/group_files/` (a command in a file, an inline group inside a file module, a `#[group]` file
  module nested one file deeper, a declared
  `#[group(ctx = T)]`, an author's own `#[path]`, a context-free file group, run-level dispatch three
  levels down, a `#[cfg]`'d-out command, and the help/details split) — plus `macros`'s own unit tests
  for `crate::resolve` against an in-memory `Fs`.
- **Curried commands and default commands (ADR-011):** `tests/curried.rs` (an `args` group's
  value reaching `#[command(curried)]` children and `#[group(curried = T)]` groups, fixed
  siblings winning their own word, aliases and hidden siblings reserved, usage with the
  placeholder, `Commands for SOURCE-NAME:` help, sibling completion by prefix) and
  `tests/default_command.rs` (the default running when no sibling is named, `Invocation::path`
  being the group's, `--` giving a literal query, the section in `--help`), new `trybuild` cases
  for each new rejection, and `tests/bpaf_contract.rs`, which pins bpaf's behaviour
  byte-for-byte (including the completion layer's reliance on `construct!` order and eager
  `ParseComp`) and is re-run on every bpaf upgrade; a future CI job should run
  `cargo update -p bpaf && just test`, since `bpaf = "0.9.27"` is a caret requirement.
- **Command errors (ADR-012):** `tests/errors.rs` (`#[command]` fns returning `eyre::Result`,
  `anyhow::Result<Success>`, `io::Result`, `Box<dyn Error>` and `Outcome` with `?`; exit codes
  through `run_with_args`; the `outer: mid: root` chain through `Invocation::run`; a usage
  failure keeping `2`; the function still returning its own `eyre::Report`), with `anyhow` and
  `eyre` as dev-dependencies only, and `trybuild` cases for a non-outcome return type, a
  `Result` of a value, and an error type that is not an error.
- **Macro-expansion equivalence:** the tree `app!`/`#[group]`/`commands!` build and the same tree
  written by hand with `App::commands(vec![…])`/`hicli::group(…)` parse identical argv to
  identical `invocation.path()`s — proving the macros are sugar, not a second code path.
- **`hicli-macros` publishes first.** `#[command]` lives in a separate, exact-pinned crate
  (`hicli-macros = { version = "=X.Y.Z", path = "macros" }`), so the release order is fixed
  by the dependency graph, not just documented in `RELEASING.md`. Before a release,
  `cargo publish --dry-run --workspace` (`just publish-dry`) packages both crates in that
  order and verifies `hicli` against the *locally* packaged macro crate; the real publish
  waits for `hicli-macros` to reach the crates.io index before `hicli` goes up.
- **External subcommands, in-process:** an `ExternalCommand` registered at a path that *cannot
  exist* is listed in `--help`, selected by name and dispatched to an `Invocation` — proving nothing is ever
  spawned outside `ExternalRun::run`. Unit tests assert the captured `rest` verbatim (flag-like args
  included; a leading `--` is consumed by bpaf and not forwarded).
- **Exit-code propagation, in-process:** the child-status → `Success`/`Failure` mapping
  (`exit_from_status`) is a pure function unit-tested with `ExitStatus`es built via
  `ExitStatusExt::from_raw` (Unix), so no subprocess is spawned; the spawn-failure path is
  exercised against the same cannot-exist path used everywhere else.
- **Run level, in-process:** `App::run_with_args(cli! { … }, mk_ctx)` exercises every branch of
  `App::run` — good parse, usage error (2), `--help` (0), a typo'd and an unknown command (2, the
  gap-closer path), `mk_ctx` failure, `Success::code(7)`→7, `Success::OK`→0, `Failure::runtime`→1,
  `Failure::usage`→2, and an external's spawn failure (1). Commands record into the shared `Ctx`, so
  each test asserts both the exit code and whether the command ran with the globals-derived context.
  Only the returned code is assertable — the streams are the process's real ones — so `did you
  mean` text remains covered by the `augment_stderr`/`suggest` unit tests.
- **Help text:** every command and group carries a mandatory one-line `help` (a doc comment, for
  `#[command]`/`#[group]`), asserted as substrings of top-level, group and leaf `--help`
  output (never whole-output snapshots); a multi-line doc comment's first line is the help, the
  rest is `details()` up to an examples heading, and the examples are the footer —
  `tests/help_examples.rs` asserts, for the indented, `# Examples` and ```` ```sh ````-fenced
  spellings in plain `--help` and `--help --help`, that the footer comes after the options and ends
  the output verbatim, and that no example text leaks into the details. `tests/metadata.rs` covers
  `about` above `Usage:`, `--version`/`-V` (stdout, exit 0), a hidden command absent from help and
  completion but parsing, and an alias dispatching under its canonical path without being listed.
- **Completion:** driven in-process through bpaf's own wire protocol
  (`--bpaf-complete-rev=1`, the revision bpaf 0.9.28 emits in its generated shell scripts, verified
  by spike), both raw (`ParseFailure::Completion`'s payload, so the wire format itself stays
  covered) and through `App::complete`, the test seam that hides the protocol entirely. The golden
  matrix includes: the top-level registry's commands and externals appear in `App::complete("")`;
  an argument type's `#[arg(complete = fn)]` candidates are reachable through a command taking it
  and nothing else, and are filtered by the partial word (the completer's job — bpaf passes a
  completer's output through unfiltered); `complete = dirs`/`complete = files` leave the parser
  itself unchanged and report no candidate values at revision 1, since a shell directive is not a
  value to list. Revision 2 is never driven through `run_inner` anywhere in the suite — verified to
  terminate the process — so `App::complete` pins revision 1 and the tests never ask for the other.
- **Globals placement:** asserted before and after the subcommand, including after a leaf's
  free-form positionals.
- **Argument order:** positionals-from-two-composed-structs are consumed in parser (= declaration)
  order — the case `EventRef` + `EventName` back to back depends on.
- `cargo publish --dry-run` succeeds for both `hicli` and `hicli-macros` (metadata + license
  complete).
- `examples/` CLI runs: good parse, usage error (exit 2 after remap), a typo (suggestion),
  `--help` at top + group level.
- **Follow-up (separate work):** adopt `hicli` in moniker (replace `Cli`/enums/`dispatch()` in
  `projects/moniker/src/main.rs`; keep `bases/cli` parser-free; prove exit-code + `.feature`
  journey parity), then mdma-cli, ctx.

## Risks & tradeoffs (honest)

- **More modular, or relocated glue?** The central match disappears; each command becomes
  colocated + independently testable; adding one is one documented function, in a module — no
  struct, no trait impl, no name written twice, and (since ADR-005) no registration line to forget
  either: membership is containment. The residual cost is a proc-macro dependency and errors that,
  when `#[derive(Bpaf)]` itself rejects an argument struct, are reported through `bpaf_derive`, one
  layer removed from `#[command]`'s own diagnostics. **Win = locality + testability + structural
  de-duplication, not zero cost.**
- **bpaf's positionals-last rule becomes a `#[command]` parameter-order rule the macro cannot
  enforce.** Argument structs are composed with `construct!` in declaration order, so a struct
  containing a positional must be the last one in the signature that has one; getting this wrong
  is a panic from `bpaf` when `--help` renders (in any build; hicli renders it once at fold time in
  debug builds so the first debug run or test hits it, #11), not a compile error from `#[command]` — it has no
  way to see inside `EventDir`/`ShareOpts` to know which is which. Mitigated by convention (declare
  switches/options first, positionals last) and by the example CLI + moniker's real argument
  structs exercising the rule, not by the macro catching it.
- **bpaf vs clap UX — two known deltas:** help text not byte-identical (target semantic
  equivalence); globals live at the top level — accepted before *and* after the subcommand, but
  with no per-subcommand redefinition or propagation the way clap's `global = true` allows.
  Documented in README.
- **crates.io name:** verify both `hicli` and `hicli-macros` are free at publish time before
  first `cargo publish`.
- **A bare path in `commands!`/`app!` is always a handle, never a local.** `commands![c]` where `c`
  is a variable already holding a `Command` looks for `__hicli_c` and fails to find it; the local
  must be written `(c)`. This is by design (ADR-005) but is a real edge a consumer hits once.
- **A `#[group]` file module is top-level-only, and depends on the compiler disclosing the source
  path.** A proc macro cannot see the inline module it sits in, so `#[group] mod b;` inside
  `mod a { … }` fails
  loudly rather than resolving against the wrong directory; under `--remap-path-prefix`, or any
  front end that doesn't implement `Span::local_file`/`Span::file`, it reports that it cannot find
  the file and points at the inline `#[group]` form instead. rust-analyzer's proc-macro server
  was observed *not* to run with the workspace root as its working directory, so the macro
  searches for its anchor (the working directory, then `CARGO_MANIFEST_DIR` and its ancestors)
  instead of assuming one; **a manual editor check that `#[group] pub mod x;` is squiggle-free is still
  outstanding** (see `ROADMAP.md`).
- **An external, or any hand-built `Command`, cannot be a discovered child of `#[group]`.** There is
  no item to mark, so its parent group needs a hand-written `__hicli_<name>` handle calling
  `hicli::group`/`commands!` itself — the handle convention is documented public API for exactly
  this case.
- **MSRV is Rust 1.99** (ADR-010): `#[group] pub mod x;` needs attribute macros on file-module
  declarations, stable since 1.99 (`Span::local_file`, stable since 1.88, is still used). Both
  crates carry `rust-version = "1.99"`; older toolchains are not supported.
- **A span from a file a `#[group]` file module reads collapses to the `#[group]` declaration.**
  `syn::parse_file` inside a proc macro loses the original `path:line` of every token in the scanned
  file (confirmed by spike); a diagnostic hicli raises about something inside `event.rs` is reported
  at the `#[group] pub mod event;` declaration, with the file and reason named in the message text,
  not in the span. rustc's own diagnostics keep their real spans, since the file is loaded by rustc,
  never inlined.
- **The completer filters; bpaf does not.** `bpaf` passes a completer's output through
  **unfiltered** (verified against 0.9.28) — it prefix-filters only the flag names it derives
  itself. A `#[arg(complete = f)]` that ignores the word it is handed offers every candidate at
  every keystroke; every completer hicli ships or documents starts with a `.starts_with(typed)`
  filter, and this is the one thing an author must remember to do by hand.
- **Non-`String` newtypes never see the empty word.** `bpaf` hands a completion function the
  **parsed** value, so a newtype over `u64` (or anything validated through `.parse(..)`) is never
  called for the empty word — the first TAB yields nothing, and there is no candidate list to
  filter. A type whose inner value does not parse from a partial word hand-writes `Arg::parser()`
  and completes at the *string* level before converting, which is why the trait is
  hand-implementable at all.
- **Revision 2 of bpaf's completion protocol exits the process when driven through `run_inner`.**
  A hole in bpaf's otherwise exit-free `run_inner` seam, outside hicli's control (verified by
  spike against 0.9.28). `App::complete` pins revision 1 for exactly this reason, and the note is
  load-bearing: "upgrading" the seam to revision 2 would take the test runner with it.
- **hicli ships no `Arg` impls — not for `String`, `u64`, `PathBuf`, and this is the feature,
  not a gap.** An impl for `PathBuf` would have to choose a metavar and positional-vs-option, and
  both choices would be silently wrong most of the time; a primitive in a command's signature
  (`fn create(name: String, ctx: &Ctx)`) is a compile error carrying
  `#[diagnostic::on_unimplemented]` notes that teach the newtype pattern at the moment it is
  broken, rather than an impl that would have made the primitive-obsessed signature compile.
- **`Globals` is not an `Arg`, and must not become one.** Globals are composed at the top level by
  `App::new`, never as a command's parameter, so they never go through `Arg::parser` — a
  `#[derive(Arg)]` on a `Globals` struct would compile and never be called from anywhere real.
