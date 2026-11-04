# ADR-004: Verbs are functions; the command tree is declared, not built

## Status

Proposed — decisions 2–4 and the "`mod create { run }`" expansion superseded by
ADR-005 (2026-09-17); decision 1 otherwise, 5 and 6 stand.

Supersedes the consumer-facing half of the `Verb` trait in `docs/PLAN.md`.
Strengthens ADR-001 (help stays mandatory, now enforced by the compiler).
Replaces `Exit` from ADR-003 with `Success` + `Failure`, keeping ADR-003's
substance: an exit code is a value and `std::process::exit` never appears in the
library.

Vocabulary renamed by [ADR-007](007-hicli-and-the-command-attribute.md) (`verbandi` → `hicli`, `#[verb]` → `#[command]`, `Verb` → `Invocation`, `VerbResult` → `Outcome`). The text below is kept exactly as written.

"Future work: group-level arguments ("currying")" realised by [ADR-011](011-curried-commands-and-default-commands.md) (2026-11-03): `#[group(args = T)]` and `#[command(curried)]`, built on a generic `Node<O>` rather than the `Mount` field sketched below.

`IntoOutcome`'s `Result` impls generalised by [ADR-012](012-command-errors-convert-through-into-failure.md) (2026-11-04): `Result<(), E>` and `Result<Success, E>` for any `E: Into<Failure>`, so `io::Result`, `anyhow::Result` and `eyre::Result` are accepted as written. The "no blanket" rule now holds for the success side only.

## Decision

### 1. A subcommand is a function

```rust
/// create an event
///
/// The event directory is created if it does not exist.
#[verb]
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> VerbResult { … }
```

- **Name** = the function ident, `_` → `-` (`add_admin` → `add-admin`);
  override with `#[verb(name = "…")]`.
- **Help** = the doc comment. First line is the one-line help *and* the
  `--help` heading; the rest is the long description. **A `#[verb]` without a
  doc comment is a compile error.**
- **Arguments** = the owned parameters. Each is a consumer-written
  `#[derive(Bpaf)]` struct — including newtype tuple structs
  (`struct Words(#[bpaf(positional("WORD"), many, guard(…))] Vec<String>)`).
  The macro calls each type's bpaf-derived constructor (snake_case of the type
  name) and composes them with `construct!`, in declaration order. **`#[verb]`
  has no parameter attributes**: it never sees an argument's spelling.
- **Context** = the single `&T` parameter, conventionally last; `T` fixes the
  verb's `Ctx`. A verb with no `&T` parameter is generic over `Ctx`. A second
  reference parameter, or a `&mut T`, is a compile error.
- **Return** = anything implementing `IntoOutcome`: `VerbResult`
  (= `Result<(), Failure>`), `VerbResult<Success>`, `Success` or `()`.
- **Expansion** = exactly two items, one per namespace: a function
  `create() -> verbandi::Command<Ctx>` (the handle) and a module `create`
  holding `create::run(…)` (the body), both inheriting the fn's visibility.
  The handle body is a call to `verbandi::command(name, help, parser, run)`.
  **(pre-ADR-005)** — ADR-005 leaves the function itself untouched and names
  the handle `__verbandi_create`; there is no `mod create { run }`.

### 2. A group is a declaration

**(pre-ADR-005 — superseded by `#[group]` on a module, decision 3 there)**

```rust
group! {
    ctx: Ctx;

    /// manage audiences
    audience: [audience::define, audience::add, audience::remove, audience::show]

    /// events and instances
    event: [create, roster, audience]
}
```

A `macro_rules!` macro in the runtime crate. Each entry expands to
`pub fn <name>() -> verbandi::Command<Ctx>` calling
`verbandi::group(name, help, commands![…])`. The name is lowercase, so groups
and verbs are indistinguishable in a list. Help comes from the doc comment,
captured as `$(#[doc = $d:literal])*`. **A child is a child because it appears
in its parent's list** — there is no registration call and no `Group::leaf`
chain in consumer code.

`ctx: <Type>;` is a per-invocation header naming the context type, because the
macro generates a `fn` signature and a `fn` cannot infer its return type. It
defaults to a type named `Ctx` in scope.

### 3. `commands![a, b, c]`

A `macro_rules!` macro building `Vec<Command<Ctx>>` from a heterogeneous list of
verb handles, group handles and externals, via

```rust
pub trait IntoCommand<Ctx> { fn into_command(self) -> Command<Ctx>; }
```

### 4. `app! { globals: …, commands: [ … ] }`

A `macro_rules!` macro expanding to
`App::new(globals()).commands(commands![…])`. `run` stays a method call, so
`main` reads `app! { … }.run(mk_ctx)`.

### 5. `leaf` → `command`

`verbandi::command(name, help, parser, run)`, `App::command`, `App::commands`,
`Command<Ctx>` (was `Leaf<Ctx>`), `verbandi::group(name, help, children)`,
`verbandi::external(ext) -> Command<Ctx>`. `Group<Ctx>` as a separate public
type is gone: a group *is* a `Command<Ctx>` with children, which is what makes
`commands![…]` homogeneous. The builder functions stay public — they are what
the macros expand to — but the documented consumer API is the macro form.

### 6. Runtime

- The `Verb` **trait** and `BoxedVerb` are removed. `Verb<Ctx>` becomes a
  **struct** — a parsed verb with its arguments already bound — carrying
  `name()`, `path()` and `run(self, &Ctx) -> Result<Success, Failure>`.
- `Exit` splits: `Success(u8)` for "finished, exit with this code" and
  `Failure { Usage(String), Runtime(String) }` for "failed, print this". An
  external subcommand's child code becomes `Ok(Success::code(n))`.
- `App::run` returns `std::process::ExitCode`; `App::run_with_args` and
  `exit_code_for` return `u8`.
- The `#[verb]` macro — and only that macro — ships in a new `verbandi-macros`
  proc-macro crate, re-exported as `verbandi::verb` and exact-version-pinned.
  `group!`, `commands!` and `app!` are `macro_rules!` in the runtime crate.

`inventory`-style auto-registration is still **not** adopted: the tree is
declared explicitly, in one place per group.

## Why

The trait model made "one subcommand = one verb" cost five artefacts: a struct,
a `Verb` impl, a name string written twice, a boxing adapter, and a registration
line carrying the name and help a third time. The *function* is the thing the
author is actually writing; everything else was scaffolding that a macro can
produce. `docs/PLAN.md` deferred proc macros "until a real consumer's pain
justifies it" — moniker is that consumer, its author is the project owner, and
the pain is stated.

- **Deleting duplication structurally, not by convention.** `fn name()` returned
  a string the registration repeated. With the name derived from the ident and
  attached by the macro, there is exactly one spelling, and no way to write a
  second.
- **Help becomes unforgettable.** ADR-001 made help a required *parameter*; a
  required doc comment is the same rule enforced one level earlier and read by
  humans in the source as well as by users in `--help`. ADR-001 rejected this
  only because it needed a proc macro.
- **No argument dialect.** With arguments as consumer-written
  `#[derive(Bpaf)]` structs, `#[verb]` never parses an argument spelling, never
  forwards a span into generated fields, and never grows `help =` sugar or a
  `#[ctx]` marker. Its whole job is: read the ident, read the doc comment, call
  one constructor per owned parameter, `construct!` them, hand the tuple to
  `verbandi::command`. That is a proc macro small enough to keep honest.
- **Argument groups become reusable types.** moniker's ~20 event commands all
  start with `EVENT_DIR` or `EVENT_DIR INSTANCE`. As a shared `EventRef` those
  positionals — and their help, their `some(…)` message, their guards — are
  written once and tested once, and every command that takes the pair says so
  in its signature. Inline parameter attributes could not do this; they would
  have re-spelled the pair at ~20 sites.
- **Groups are data.** `fn event() -> Commands { commands![…] }` made every
  group a hand-written function whose only content was a list; a builder chain
  (`Group::new(…).leaf(…).group(…)`) made it a hand-written function whose only
  content was a list plus punctuation. The list is the decision; `group!` writes
  it down and nothing else. Reading `event: [create, roster, audience]` answers
  "what is under `event`?" in one line, which the old `.leaf(…).leaf(…)` chain
  needed a scroll to answer.
- **One word for one concept.** `leaf` was a tree-shape word in an API whose
  whole vocabulary is otherwise command-shaped (`command(name)` in bpaf,
  `Command<Ctx>`, `commands![…]`, `app <cmd> --help`). With groups and leaves
  unified behind `Command<Ctx>`, keeping `leaf` would have named a distinction
  that no longer exists.
- **`Verb` as a struct is smaller than `Verb` as a trait.** A trait object with
  an inherent-looking `name()` existed only so tests could ask "which verb was
  selected". A struct answers that better (`path()` also disambiguates
  `event create` from `id create`), needs no `Box<dyn …>` alias in the public
  API, and leaves the closure to carry state.
- **`Success`/`Failure` removes a wart ADR-003 accepted under protest.**
  `Exit::Code(0)` normalising to `1` existed only because a *success* code had
  nowhere to live. With the split it does: an external's child exiting 7 is
  `Ok(Success::code(7))`, silent because a `Success` has no message to print —
  structural, where ADR-003 needed `message() -> Option<&str>` plus a rule. The
  failure enum shrinks to two variants and its `message()` loses its `Option`.
  `u8` makes `Exit::Code(256)` — a failure that POSIX would report as success —
  unrepresentable.
- **`ExitCode` from `run`.** With codes bounded to `u8`, `App::run` can return
  `std::process::ExitCode` and consumer `main` loses its `try_from(…).unwrap_or(1)`
  line. `run_with_args` keeps returning `u8` because `ExitCode` is not
  comparable and a test seam must be assertable.

## Alternatives considered

- **`#[arg(...)]` parameter attributes** (the first revision of this ADR):
  `#[verb] fn create(#[arg(positional("EVENT_DIR"))] event_dir: String, …)`.
  Rejected. It forced the macro to generate a `#[derive(Bpaf)]` struct, which
  means forwarding each parameter's span so `bpaf_derive`'s errors point at
  source the author wrote; it introduced a verbandi-owned attribute dialect
  (`help = "…"`, `short = "d"`) parallel to bpaf's; it had no place to put a
  doc comment on a parameter, so help strings became attribute values; and it
  made a shared argument group like `EventRef` inexpressible, re-spelling
  moniker's `EVENT_DIR INSTANCE` pair at every one of ~12 sites. Owned
  `#[derive(Bpaf)]` parameters delete all four problems at once and shrink the
  proc macro to name + doc + constructor calls.
- **A consumer-facing builder for groups**, `Group::new(name, help).leaf(…).group(…)`.
  Rejected. It is a function whose body is a list, so every group costs a `pub
  fn`, a `use`, and a chain of punctuation; the parent–child relation is spread
  across method calls rather than readable as one line; and it keeps `Group` as
  a second public type that `commands![…]` would then have to unify over.
  `group!` is strictly less to write and less to read, and `verbandi::group(…)`
  survives as the primitive it expands to.
- **`fn event() -> Commands { commands![create, roster, audience] }`.**
  Rejected as redundant: the function, its name, its return type and its `use`
  all restate what the one-line declaration already says.
- **Keep the trait, add `#[verb]` as sugar over it.** Two ways to declare a
  verb, the longer one privileged by being the "real" API, and `fn name()`
  survives as a second place to write the name. Rejected: the duplication the
  change exists to remove would remain reachable.
- **A `#[ctx]` marker attribute instead of "the `&T` parameter".** More
  explicit, one more thing to learn and to forget. Reference-ness already
  distinguishes context from arguments, and with no other parameter attributes
  existing, adding one solely for context would reintroduce the attribute
  surface this revision deleted.
- **Naming each argument struct's constructor explicitly** rather than deriving
  it as snake_case of the type. Rejected for v1: it is either a parameter
  attribute (deleted) or a `#[verb(args(…))]` list that restates the parameter
  types. A type whose bpaf constructor has a different name is registered with
  `verbandi::command(…)` by hand.
- **Interpret argument attributes ourselves and emit bpaf combinators.** Moot
  now — with no attributes there is nothing to interpret — but it also remains
  the fallback if `#[derive(Bpaf)]` proves unusable from a crate that does not
  depend on `bpaf` directly.
- **`#[verbs]` on a module for groups.** It cannot be applied to `mod event;`
  (a file module gives the macro no body), and moniker's layout is one file per
  group. `group!` works in either layout.
- **`inventory`/`linkme` auto-registration.** Still rejected: link-time magic,
  no explicit tree to read, and it breaks "adding a command is a new file plus
  one registration line" in the direction of "adding a command is invisible".
- **Keep `Exit` unchanged and let `#[verb]` map onto it.** Preserves ADR-003
  verbatim but keeps `Code(0) -> 1` and leaves "success with a code"
  inexpressible, which is precisely what the owner's
  `Result<CommandResultCode, …>` asks for.

## Consequences

- **Breaking, wholesale, against the in-repo 0.1.0.** The crate is unpublished
  with zero external consumers. In-repo call sites: `src/app.rs`, `src/verb.rs`,
  `src/external.rs`, `src/lib.rs` doc example, `src/macros.rs` doc example,
  `examples/demo.rs`, `tests/cli.rs`, `README.md`, `docs/PLAN.md`.
- **`verb_test!` and `cli!` are unchanged**, including their syntax. The closure
  now receives `Result<(G, Verb<Ctx>), ParseFailure>`, and identifies the
  selected verb through the inherent `name()` / `path()`.
- **A second crate to publish**, `verbandi-macros`, exact-pinned and published
  first. `cargo publish --dry-run` for `verbandi` cannot verify until the macro
  crate is on crates.io.
- **No new third-party dependencies**: `syn`/`quote`/`proc-macro2` are already
  in the tree via `bpaf_derive`.
- **Consumers add a direct `bpaf` dependency.** Verified 2026-09-16:
  `#[derive(Bpaf)]` expands to `::bpaf::…` paths, which only resolve against a
  crate's own extern prelude, and `use verbandi::bpaf;` does not help. One line,
  `bpaf = "0.9"`, with no features (unification with verbandi's dependency
  supplies `derive`). verbandi documents the required version; the "pin one
  bpaf through verbandi" claim in `docs/PLAN.md` weakens to "verbandi states
  the version, cargo unifies it".
- **A group must name its context type once**, in the `ctx:` header, because it
  expands to a `fn` item. `app!` and `commands!` are expressions and infer it.
- **Group declarations live in the module that owns the children**, so a nested
  group's children are referenced by path (`audience::define`). A group's `fn`
  and a same-named `mod` coexist, as a verb's handle and module already do.
- **`group!` bodies are not rustfmt-formatted**, because `name: [a, b]` is not
  Rust syntax at any delimiter. The canonical layout is documented and covered
  by the example CLI.
- **Verb bodies referring to their own name recursively** now resolve to the
  generated handle function, not the body. Rare, and it fails as a type error.
- **Currying stays additive.** See below.

## Future work: group-level arguments ("currying")

`moniker event ./ev main create "Summer Party"` — the `event` group parses
`EventRef` once and every child receives it as one of its parameters. The
grammar has a free slot for it:

```rust
group! {
    ctx: Ctx;

    /// events and instances
    event(EventRef): [create, roster, audience]
}
```

**Superseded by ADR-005:** with the list-form `group!` gone, the syntax slot
moves to `#[group(args = T)]` on the module declaration, beside `ctx = …`,
rather than a position in this bespoke grammar. The rest of this section — the
three primitive shapes chosen so currying stays additive — still holds.

This is **not** built in v1, and three shapes are chosen now so it can be added
without breaking anything. (i) A `Command<Ctx>` does **not** store a finished
`bpaf` parser; it stores a deferred builder, `FnOnce(Mount) -> Box<dyn
Parser<Verb<Ctx>>>`, where `Mount` is a private `#[non_exhaustive]` struct
carrying the parent's path prefix. Currying adds a field to `Mount` (the
parent's parsed value / its parser) rather than changing a signature.
(ii) `Verb<Ctx>::run` stays `FnOnce(&Ctx) -> Result<Success, Failure>` with
every argument bound at *parse* time — a curried argument is bound the same way,
so `Verb` never changes and the run path never learns about groups. (iii) A
verb's body is `create::run(dir, name, ctx)` either way: currying changes where
`dir` came from, not the function. `IntoCommand<Ctx>`, `App`, `commands!` and
`app!` are untouched; only `verbandi::command`/`verbandi::group` gain a variant
and `#[verb]` learns to leave one parameter's parser slot open.
