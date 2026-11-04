# hicli — architect context

`hicli` is a standalone, crates.io-publishable Rust **library crate** (not Polylith,
not a workspace) that provides a modular CLI framework on top of `bpaf`. The north star
is in `VISION.md`; the full API design, rationale, and `bpaf` capability analysis are in
`docs/PLAN.md` — **read `docs/PLAN.md` before reviewing any non-trivial change.**

## The core idea (what every review defends)

One subcommand = one documented function (ADR-004: `docs/adr/004-function-first-verbs.md`;
ADR-005: `docs/adr/005-verbs-are-the-function.md`, which supersedes ADR-004's group/list
mechanics). A `#[command]`-annotated `fn create(dir: EventDir, name: EventName, ctx: &Ctx) ->
Outcome { … }` *is* the command, and `#[command]` leaves it **exactly as written** — same name,
signature, visibility, body — adding only a hidden sibling handle
(`__hicli_create() -> Command<Ctx>`) beside it: its name from the ident, its help from the doc
comment, its arguments from its owned parameters, its context from its one `&T` parameter. A group
is an annotated **module**, not a list: `#[group]` on an inline `mod event { … }` discovers every
`#[command]`/`#[group]` item in its body, in source order — a child is a child because it is in the
module, no registration line to keep in step. A **file** module is the same group spelled
`#[group] pub mod event;` (ADR-010: `docs/adr/010-file-groups-are-an-attribute.md`; it replaced the
pre-1.99 `group_mod!` shim, which no longer exists). Commands compose into a tree by declaration (`app!`/`commands!`), **no central
hand-written dispatch `match`, and no consumer-implemented trait.** If a change reintroduces a
growing central dispatcher, brings back a *trait* a consumer implements per command, reintroduces a
list that restates the module's contents (the ADR-004 `group! { event: [create, roster] }`
shape), or makes adding a command heavier than "a new `#[command]` function in the right module,"
that is a design regression worth calling out.

**An argument is a type** (ADR-006: `docs/adr/006-an-argument-is-a-type.md`). Each owned
parameter is a consumer-written type implementing `hicli::Arg` — ordinarily
`#[derive(Bpaf, Arg)]` beside each other, `Arg` writing one impl that calls the constructor
`#[derive(Bpaf)]` already generated. `#[command]` emits `<T as Arg>::parser()` for each owned
parameter, never a name it synthesises. Completion is not a second concept: `#[arg(complete = f
| files | dirs)]` on the type attaches a completer to the parser the type already returns, so
every command taking that type inherits its completion for free. If a change reintroduces a blanket
`impl<T: Bpaf> Arg for T`, ships an `Arg` impl for a primitive (`String`, `u64`, `PathBuf`, …), or
moves completion back to being something written per-command, that is a design regression worth
calling out.

## Crate layout

- `src/outcome.rs` — `Success(u8)`, `Failure::{Usage, Runtime}`, `Outcome`, `IntoOutcome`
  (four shapes: `()`, `Success`, `Result<(), E>`, `Result<Success, E>`; the error side is
  generic over `E: Into<Failure>` via a blanket `From<E: Into<Box<dyn Error>>> for Failure`,
  ADR-012; `Failure` never implements `Error`). Replaces the
  old `Exit` enum (ADR-004, amending ADR-003): a success code now lives on the `Success` side, so
  there is no `Code(0)` → `1` normalisation and `Failure::message()` is never `Option`.
- `src/invocation.rs` — `Invocation<Ctx>`, a **struct** (not a trait): a parsed command with its arguments
  already bound, carrying `name()`, `path()` and `run(self, &Ctx) -> Result<Success, Failure>`.
  Never built by a consumer directly — it's what a `Command`'s parser produces.
- `src/command.rs` — `Node<O>`, the *only* node type, generic over what it parses to (ADR-011):
  `Command<Ctx> = Node<Invocation<Ctx>>` (a leaf, a group and an external are one type, which is
  what makes `commands![…]` homogeneous) and `Curried<T, Ctx> = Node<Partial<T, Invocation<Ctx>>>`
  (a command awaiting the value its `#[group(args = T)]` parses). Built via the free functions
  `command`/`group`/`external` plus `curried_command`/`group_args`/`Node::by_default`; `IntoCommand<Ctx>` (a `Command` or a zero-arg callable returning
  one — a `#[command]`/`#[group]` handle).
- `src/app.rs` — `App<G, Ctx>` composition builder (`command`/`commands`/`run`), the
  `into_parser`/`parse`/`run_with_args`/`complete` test seams, `exit_code_for`, and the `strsim`
  suggestion gap-closer. `App::run` returns `std::process::ExitCode`; the test seams stay `u8`
  (`complete` returns `Vec<String>`). `complete` pins `bpaf`'s completion protocol at revision 1 —
  revision 2 driven through `run_inner` terminates the process, a hazard outside hicli's
  control (ADR-006).
- `src/external.rs` — `ExternalCommand`/`ArgSchema`/`ArgSpec` (git-style multi-binary model); a
  child's exit code comes back as `Ok(Success::code(n))`, never `std::process::exit`.
- `src/arg.rs` — `Arg` (one required method, `fn parser() -> impl Parser<Self>`, with
  `#[diagnostic::on_unimplemented]` notes), `Candidate` (`= (String, Option<String>)`), and the
  completion helpers `complete_with`/`complete_files`/`complete_dirs` (ADR-006). Ships no `Arg`
  impls for anything.
- `src/macro_support.rs` — **gone (ADR-005).** The dash rule and the doc-comment split it used to
  hold now live once, in the proc-macro crate (`macros/src/shared.rs`), because a proc macro can
  read a doc comment's text and rewrite idents; a `macro_rules!` macro could not, which is why this
  module existed at all.
- `macros/` — a separate, exact-pinned `hicli-macros` proc-macro crate, re-exported as
  `hicli::{command, group, commands, Arg}`. `proc_macro::TokenStream` appears only in
  `macros/src/lib.rs`; everything else works in `proc_macro2` so it unit-tests outside a macro
  expansion:
  - `shared.rs` — the dash rule and doc-comment split `#[command]` and `#[group]` both use.
  - `parse.rs` / `expand.rs` — `#[command]`: parse the function, emit it verbatim plus its handle.
    Since ADR-006, `expand.rs` binds one local per owned parameter to
    `<T as ::hicli::Arg>::parser()` and composes the locals with `construct!`, rather than
    synthesising a bare constructor call by name; `parse.rs` no longer derives a snake_case name
    from a parameter's type at all — that rule moved into `macros/src/arg.rs`.
  - `discover.rs` — which items in a module body are a group's children (a `#[command]` fn, an
    inline `#[group]` mod, or a `#[group] mod x;` declaration), in source order.
  - `ctx.rs` — a group's `Ctx`: `#[group(ctx = T)]`, else the first `&T` in a depth-first walk of
    the children (crossing file boundaries via `Outline`), else generic.
  - `group.rs` — `#[group]`'s expansion: the module/declaration plus one handle in the parent
    scope; routes the file form and the inline form through the same code from the point the items
    are in hand. The file form re-emits the declaration with a relative `#[path]` for rustc to load
    — it never inlines the file's contents.
  - `fs.rs` — `trait Fs` + `RealFs` + an in-memory `Fs` for unit tests. `anchor` searches for the
    declaring file — the path itself if absolute, then under rustc's cwd (the workspace root
    under cargo), then `CARGO_MANIFEST_DIR` and each of its ancestors — because rust-analyzer's
    proc-macro server does not run from the workspace root. The winning directory becomes the
    `Fs` root, so emitted `#[path]` values and diagnostics stay relative.
  - `resolve.rs` — the Reference's mod-rs/non-mod-rs file-resolution rules, against `&dyn Fs`.
  - `commands.rs` — `commands!`: a bare path's last segment becomes `__hicli_<ident>`; anything
    else is an expression through `IntoCommand`.
  - `arg.rs` — `#[derive(Arg)]` (ADR-006): one impl calling the type's snake_case `bpaf`
    constructor (shared `shared::snake_case`, spanned at the type's ident), optionally wrapped in
    `complete_with`/`complete_files`/`complete_dirs` from `#[arg(complete = fn | files | dirs)]`.
    Rejects a multi-field struct or an enum under `#[arg(complete = …)]` (a whole-value completer
    cannot tell which field is mid-word), a union, and a generic type.
  `app!`, `cli!`, `cli_test!` stay `macro_rules!` in the runtime crate's `src/macros.rs`; `app!`
  forwards its command list as `$($c:tt)*`, not `$c:expr`, so `commands!`'s bare-path rewrite still
  sees bare paths.
- `src/testing.rs` — arg-vector helpers backing the macros.
- `examples/demo.rs`, `tests/cli.rs`, `tests/groups.rs`, `tests/group_files.rs` (+
  `tests/group_files/` fixtures), `tests/macros.rs`, `tests/arg_derive.rs`, `tests/ui/` (the
  trybuild suite) — living docs + the in-process test matrix + every compile-time rejection.
  `examples/files/{main,event}.rs` (`[[example]] files`, `just demo-files`) exercises
  `#[group] pub mod event;` across two files; `examples/demo.rs` stays single-file and inline so both forms are living
  documentation.

## The consumer shape

A command is a function, left exactly as written; the macros are sugar over public primitives, never
the only way to do something:

```rust
/// create an event
#[command]
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome { … }
```
expands to the function **verbatim** plus one hidden sibling in the value namespace,
`__hicli_create() -> Command<Ctx>` (always `pub`, `#[doc(hidden)]`), calling
`hicli::command(…)` — both public API, so a command that outgrows the macro is rewritten with
`command()` by hand and nothing is lost; `create` itself stays callable by name, by its own tests,
by its neighbours, and recursively by itself. A group is an annotated **module**, not a list:
```rust
/// events and instances
#[group]
pub mod event {
    /// create an event
    #[command]
    pub fn create(…) -> Outcome { … }
}
```
discovers `create` (and any nested `#[group]`) by source order and emits `__hicli_event()`
beside the module — a child is a child because it's in the module, no registration call. A **file**
module is `/// events and instances` + `#[group] pub mod event;` (keys `ctx`/`alias`/`hide` on the
attribute); `#[group]` reads and parses `event.rs` (resolved by the Reference's mod-rs/non-mod-rs
rules) and emits the declaration plus the same shape of handle. `commands!`/`app!` assemble the tree —
a bare path is always a handle (`create` → `__hicli_create()`); consumers add a direct
`bpaf = "0.9"` dependency (`#[derive(Bpaf)]` expands to `::bpaf::…` paths that only resolve against
a crate's own extern prelude — `use hicli::bpaf` does not help), so "consumers pin one `bpaf`
version through hicli" is now "hicli states the version, cargo unifies it." See
`docs/sketches/moniker.rs` for a full consumer tree in this shape.

`EventDir`/`EventName` above are consumer-written newtypes deriving both `Bpaf` and `Arg`
(ADR-006) — the type *is* the argument, completion included:
```rust
/// `ID` — an identity in this home.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_ids)]        // fn(&Id) -> Vec<Candidate>
pub struct Id(#[bpaf(positional("ID"))] pub String);
```
Every command taking an `Id` inherits `Id`'s completion with nothing written at it; `#[command]`
expands to `let a0 = <Id as ::hicli::Arg>::parser();` bound before `construct!`, never
`id()`/`<name>()` written by hand or inferred from an argument's spelling.

## Design contracts to hold the line on

- **`Failure` never implements `std::error::Error`.** The blanket `From` (ADR-012) coheres only
  because of it. Any error becomes `Runtime` (code 1, rendered `outer: mid: root`) and is never
  downcast back to a code — usage errors come from the `Arg` parser or an `Outcome`-returning
  command.

- **`#[command]` leaves the function alone.** Same name, signature, visibility, attributes, body —
  verbatim. It only *adds* a hidden sibling handle. A change that makes the macro rewrite the
  function's own body, rename it, or wrap it in a synthetic module (`mod create { run }`, ADR-004's
  shape) is a regression ADR-005 exists to prevent.
- **The `__hicli_<ident>` handle convention is public API, not an implementation detail.** It is
  always `pub` + `#[doc(hidden)]`, regardless of the item's own visibility, because a group's handle
  must reach one level down into a possibly-private child. `commands![a::b]` and a hand-written
  `#[doc(hidden)] pub fn __hicli_x()` both rely on the name being stable and documented.
- **`#[command]` never sees an argument's spelling.** Arguments are owned parameters, each a
  consumer-written type implementing `Arg`, ordinarily `#[derive(Bpaf, Arg)]`; the macro's whole
  job is name + doc + one `<T as Arg>::parser()` call per parameter, composed with `construct!`.
  No `#[arg(...)]` parameter-attribute dialect on the function — that was tried and rejected
  (ADR-004's Alternatives); `#[arg(complete = …)]` is a *type*-level attribute beside
  `#[derive(Bpaf)]`, never on a command's parameter.
- **`#[command]` emits a trait call, not a name (ADR-006).** The generated handle binds
  `<T as ::hicli::Arg>::parser()` to a local per owned parameter and composes the locals with
  `construct!` — it does not synthesise a bare `snake_case()` call and does not know or care what
  a type's `bpaf` constructor is named. A change that makes `#[command]` go back to deriving a
  constructor name from a parameter's type is the regression ADR-006 moved into `#[derive(Arg)]`
  on purpose.
- **hicli ships no `Arg` impls.** Not for `String`, `u64`, `PathBuf`, or anything else — a
  primitive in a command's signature is a compile error by design, carrying
  `#[diagnostic::on_unimplemented]` notes that teach the newtype pattern at the point it breaks.
  A blanket `impl<T: Bpaf> Arg for T` is impossible besides (a derive emits a free function, not a
  trait impl, so there is nothing to key a blanket impl on) and would forbid the hand-written
  impls that are the escape hatch even if it existed.
- **The completer filters; completion completes what already parses.** `bpaf` passes a
  completer's output through unfiltered (it prefix-filters only its own flag names), so every
  completer hicli ships or documents filters by the partial word itself. `bpaf` also hands a
  completer the *parsed* value, never the raw word, so a type that does not parse from a partial
  string (a `u64` newtype, say) never sees the empty word at all; such a type hand-writes
  `Arg::parser()` and completes at the string level before converting — the same relationship
  `#[command]` has to `hicli::command`.
- **`App::complete` pins `bpaf`'s completion protocol at revision 1, and that is permanent, not
  provisional.** Revision 2 driven through `run_inner` terminates the process (verified against
  bpaf 0.9.28) — a hole in bpaf's otherwise exit-free seam, outside hicli's control. A change
  that "upgrades" the seam to revision 2 would take the test runner, and any consumer's test suite,
  down with it.
- **`#[group]` discovers; it never takes a list.** A child is a child because it carries `#[command]`
  or `#[group]` and is in the module body, in source order. Reintroducing a list that restates
  membership (ADR-004's `event: [create, roster]`) is the exact regression ADR-005 removed.
- **A file `#[group]` reads the file; it never runs code to find one, and never inlines it.** It
  resolves a `#[group] mod x;` declaration to a path with the Reference's own rules, reads and
  `syn::parse_file`s it, and re-emits the declaration with a relative `#[path = "…"]` so rustc reads
  exactly the bytes the macro read (spans, rust-analyzer and dep-info stay real). A file group must
  be at its file's top level: `#[group] mod b;` inside an inline `#[group] mod` without its own
  `#[path]` is a compile error. MSRV is Rust 1.99 (attribute macros on out-of-line modules;
  `group_mod!` is gone, ADR-010).
- **`proc_macro::TokenStream`/`proc_macro::Span` appear only in `macros/src/lib.rs`.** Every other
  module in `macros/` works in `proc_macro2`/`syn`, precisely so it can be unit-tested outside a
  macro expansion (`crate::resolve` against an in-memory `Fs` is the clearest example).
- **Help is a mandatory doc comment, not an optional string.** A `#[command]`/`#[group]` item with no
  doc comment, or a blank one, is a compile error (ADR-001, enforced by rustc reading the doc
  comment's text directly — no runtime panic, unlike ADR-004's list-form `group!`).
- **`std::process::exit` never appears in the library.** A command *returns* its outcome —
  `()`/`Success`/`Result<(), E>`/`Result<Success, E>`, normalised via `IntoOutcome` — and
  `App::run` turns that into a `std::process::ExitCode` it returns, never one it exits with.
- **Secrets never become parser fields.** Context (incl. anything secret) is built in the
  consumer's `mk_ctx` closure from the environment, not from `bpaf` `env()` args.
- **Testability is in-process, at every layer.** An argument type's own `Arg::parser()` (tested
  once per shared type), the command function itself called the way it is declared (`create(dir, name,
  &ctx)` — not through a handle, not through a synthetic `run`), `App::parse` (parse + dispatch),
  `App::run_with_args` (the whole run path) and `App::complete` (completion, pinned at protocol
  revision 1 — ADR-006) — no subprocess anywhere, and the `cli!`/`cli_test!` macros are unaffected
  by ADR-005/ADR-006, including their syntax.
- **External-subcommand discovery is trust-conscious.** No implicit `$PATH` scan;
  allowlist by absolute path; no implicit secret forwarding to child binaries.

## Guiding principle

**Subtractive design.** The value is a *smaller, clearer* way to build a CLI. `#[command]` itself was
deferred until a real consumer's pain (moniker's ~330-line dispatch, five artefacts per command)
justified the proc-macro dependency, and `#[group]`/`Arg`+`#[derive(Arg)]` were
earned the same way — the last one closing off completion, which was unreachable while a command's
parsers were synthesised inside `#[command]`'s own expansion. Group-level currying
(`#[group(args = T)]`, `#[group(curried = T)]`, `#[command(curried)]`) and a group's
`#[command(default)]` were earned by mdma-cli's migration (ADR-011) — that is the bar for adding more.
Currently deferred: an inner `#![group]` (still E0658, rust-lang/rust#54726); nested `args`
groups, a default command in a curried group or at the top level, and a dedicated
`source NAME --help` page (ADR-011); `inventory`/`linkme` auto-registration (rejected outright, not just deferred: link-time magic with
no explicit tree to read — discovery here is lexical and local, one module body or one file,
readable by eye); and, per ADR-006, a field-composing `#[derive(Arg)]` for composite types (a
composite hand-writes `Arg::parser()` instead — six lines), `#[arg(parser = …)]` for a renamed
`bpaf` constructor (a three-line hand-written impl covers it), completion masks on the shell
shorthands (`complete = files("*.toml")`), and globals-aware completion (needs a type-erased
process-global stash with a runtime-fallible downcast hicli cannot synthesise). Don't
front-load any of these — earn them. API surface should be minimal and hard to misuse.

## Delegation workflow

The architect is **read-only**: review and advise, never edit. Implementation goes to
**code-minions**; multiple can run in parallel on disjoint files. When a change is
significant enough to warrant a recorded decision, say "this warrants an ADR" (ADRs live
under `docs/adr/`).

## Rust specifics

Standard rust-architect references apply — especially `type-driven-design.md`,
`error-handling.md`, `testing.md`, `patterns.md`, and `lifetimes.md` (loaded on demand by
the agent). `async-tokio.md`, `embedded.md`, `ecs-beyond-games.md`, and `polylith.md` are
not applicable to this crate. Run `clippy_new_warnings` at the start of any Rust review;
the crate builds/tests via `just ci` (fmt-check + clippy `-D warnings` + tests).
