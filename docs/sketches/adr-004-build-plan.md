# ADR-004 build plan (working document, not part of the crate)

Architect's implementation plan for the function-first rewrite, 2026-09-15.
Each step is one code-minion, `just ci` green before the next. Read
`docs/adr/004-function-first-verbs.md` and `docs/sketches/moniker.rs` first.

## Step 0 — spike (throwaway crate, nothing committed)

| # | Question | Fallback if the answer is bad |
|---|---|---|
| 0.1 | Does `#[derive(Bpaf)]` expand to `::bpaf::…` absolute paths? | Consumers add a direct `bpaf` dependency, version-documented by verbandi; or `#[verb]` emits combinators instead of relying on the derive. |
| 0.2 | Does `$(#[doc = $d:literal])*` capture `///` comments in `macro_rules!`? (`$(#[$m:meta])*` captures them but the string cannot be extracted from a `meta`; match the `doc = literal` form.) | `group!` takes `"manage audiences"` as a string after the name. |
| 0.3 | Does `construct!(p)` with arity 1 compile, and does it yield `T` or `(T,)`? | `#[verb]` special-cases arity 1 to use the constructor directly (planned anyway). |
| 0.4 | Does `bpaf_derive` support newtype tuple structs, and does a doc comment on the unnamed field become help? | Named single-field structs — costs `.dir` at every use site. |
| 0.5 | Does `guard(...)` applied after `many` guard the `Vec` (not each element)? | Guard in a `parse(…)` combinator, or `.guard()` on the composed parser inside the struct's ctor. |
| 0.6 | Does coherence accept `impl<Ctx, F: FnOnce() -> Command<Ctx>> IntoCommand<Ctx> for F` alongside `impl<Ctx> IntoCommand<Ctx> for Command<Ctx>`? | Keep only the callable blanket impl; a bare `Command<Ctx>` is registered with `App::command`, externals are wrapped in a handle fn. The sketch already assumes this stricter fallback. |
| 0.7 | Does rustfmt touch `group! { … }` bodies? (Expected: no, at any delimiter.) | Accept as a documented papercut; keep braces. |

### Step 0 results (2026-09-16, spike crate in the session scratchpad)

| # | Answer | What it means for the build |
|---|---|---|
| 0.1 | **No.** `#[derive(Bpaf)]` emits `::bpaf::…` paths; `use verbandi::bpaf;` does not help. A direct `bpaf = "0.9"` line (no features needed, feature unification supplies `derive`) fixes it. | Consumers add one dependency line. Document the required `bpaf` version in README and `docs/PLAN.md`; the "emit combinators" fallback is not needed. |
| 0.2 | **Yes.** `$(#[doc = $d:literal])*` captures `///` lines. Each literal has a leading space; a blank `///` line yields `""`. Extracting the first literal needs an internal `@first` rule. | `group!` trims each literal and skips the empty separator when splitting help from details. |
| 0.3 | **Arity 1 is a trap.** `construct!(a())` compiles but yields a parser-of-parser and fails later with a confusing `Clone` error. `construct!(a(),)` with a trailing comma yields plain `A`. `construct!(a(), b())` yields `(A, B)` in declaration order. `construct!(())` does not compile. | `#[verb]` emits `construct!($($ctor()),* ,)` uniformly for arity ≥ 1 and `pure(())` for arity 0. Never emit the bare arity-1 form. |
| 0.4 | **Yes.** Newtype tuple structs derive, the ctor is snake_case of the type, a doc comment on the unnamed field shows in `--help`. | Sketch stands. |
| 0.5 | **Yes.** `guard` after `many` guards the `Vec`; before `many` it guards each element. Order decides, both compile. | Sketch stands; document the ordering rule. |
| 0.6 | **Yes.** The `FnOnce() -> Command<Ctx>` blanket impl coexists with the impl for `Command<Ctx>`. | Both `IntoCommand` impls ship; bare `Command` values are accepted in `commands![…]`. |
| 0.7 | **rustfmt leaves `group! { … }` bodies untouched** at every delimiter. | Documented papercut, braces kept. |

Verified in step 2 rather than step 0 (needs the real fold):
- Positionals from two `construct!`-composed derived structs are consumed in parser order.
- A `command` parser composed after positionals still parses (only the future currying work needs this — record, do not act).

## Step 1 — outcomes (`src/outcome.rs`, replaces `Exit` in `src/verb.rs`)

`Success(u8)` (`Success::OK`, `Success::code(n)`, `code()`), `Failure::{Usage, Runtime}` (`usage`, `runtime<E: Display>`, `code() -> u8`, `message() -> &str`, `Display`), `type VerbResult<T = ()> = Result<T, Failure>`, `trait IntoOutcome` with four concrete impls (`()`, `Success`, `Result<(), Failure>`, `Result<Success, Failure>` — no blanket impls).

Tests: codes 2/1/n; `Success::code(0)` is a legal success and stays 0; `Failure` has no silent variant; each `IntoOutcome` impl round-trips. Delete `exit_code_zero_is_not_success`.

## Step 2 — the command tree (`src/command.rs`, replaces `Group` in `src/app.rs`)

```rust
pub struct Verb<Ctx> { name: &'static str, path: Vec<&'static str>, action: Box<dyn FnOnce(&Ctx) -> Result<Success, Failure>> }
impl<Ctx> Verb<Ctx> { pub fn name(&self) -> &'static str; pub fn path(&self) -> &[&'static str]; pub fn run(self, ctx: &Ctx) -> Result<Success, Failure>; }

pub struct Command<Ctx> { name: &'static str, help: String, details: Option<String>, kind: Kind<Ctx> }
enum Kind<Ctx> { Leaf(Box<dyn FnOnce(Mount) -> Box<dyn Parser<Verb<Ctx>>>>), Group(Vec<Command<Ctx>>), External(ExternalVerb) }
#[non_exhaustive] struct Mount { path: Vec<&'static str> }   // private; the currying seam

pub fn command<Ctx, A, R>(name: &'static str, help: impl Into<String>,
                          parser: impl Parser<A> + 'static,
                          run: impl Fn(A, &Ctx) -> R + Clone + 'static) -> Command<Ctx>
    where A: 'static, R: IntoOutcome, Ctx: 'static;
pub fn group<Ctx>(name: &'static str, help: impl Into<String>, children: Vec<Command<Ctx>>) -> Command<Ctx>;
pub fn external<Ctx>(ext: ExternalVerb) -> Command<Ctx>;
impl<Ctx> Command<Ctx> { pub fn details(self, d: impl Into<String>) -> Self; pub fn name(&self) -> &'static str; }

pub trait IntoCommand<Ctx> { fn into_command(self) -> Command<Ctx>; }
```

`Kind::Leaf`'s closure is deferred so the verb's `path` is baked in at mount time and `Mount` can grow. `run` needs `Clone` because bpaf's `map` takes `Fn`; the generated closure is non-capturing.

Tests: a hand-built one-command tree parses and dispatches; `path()` is `["event", "create"]` nested and `["apply"]` top-level; a group with no children panics with the existing message; `command` accepts each `IntoOutcome` return shape; positional ordering across two composed argument structs.

## Step 3 — `App` (`src/app.rs`)

`App::command(impl IntoCommand<Ctx>)`, `App::commands(impl IntoIterator<Item = Command<Ctx>>)`; drop `App::leaf`/`App::group`/`App::external`. `names` derived from the top-level `Vec<Command<Ctx>>` at fold time. `into_parser -> OptionParser<(G, Verb<Ctx>)>`; `run -> ExitCode`; `run_with_args -> u8`; `exit_code_for -> u8`; `report_exit` becomes `report_outcome(Result<Success, Failure>) -> u8`.

Tests: the whole `tests/cli.rs` run-level block re-expressed — `Success::code(7)` → 7, `Success::OK` → 0, `Failure::runtime` → 1, `Failure::usage` → 2, `mk_ctx` failure, gap-closer path. Keep every `suggest`/`augment_stderr` unit test. Add: `run` returns `ExitCode::from(run_with_args(…))`.

## Step 4 — `verbandi-macros` (`#[verb]` only)

New proc-macro crate under `macros/`, exact-pinned, re-exported as `verbandi::verb`. Workspace `Cargo.toml` gains `[workspace] members = [".", "macros"]`. Expansion is exactly the sketch's reference block. Errors, each with a `trybuild` case: no doc comment; two reference parameters; a `&mut` parameter; `self`; a generic or lifetime-bearing signature.

Tests: `trybuild` UI suite for the five errors; expansion tests covering arity 0/1/n, no-`&Ctx` (generic over `Ctx`), `pub` vs private visibility, `add_admin` → `add-admin`, `#[verb(name = "…")]`, each `IntoOutcome` return type.

## Step 5 — `group!` / `commands!` / `app!` (`src/macros.rs`)

`group!` takes an optional `ctx: <Type>;` header (default `Ctx`) and one or more `$(#[doc = $d:literal])* $name:ident : [ $($child:path),* $(,)? ]` entries.

Tests (`tests/macros.rs`): `group!` with and without the header; multi-group invocation; multi-line doc comment (first line help, rest details); path children; trailing commas; `commands!` mixing a verb, a group and an external handle; `app!` producing the same tree as the builder chain (parse the same argv through both, compare `verb.path()`).

## Step 6 — `src/lib.rs`, `examples/demo.rs`, `tests/cli.rs`, `README.md`

Re-export surface: `bpaf`, `verb`, `app`, `group`, `commands`, `cli`, `verb_test`, `App`, `Command`, `IntoCommand`, `Verb`, `Success`, `Failure`, `VerbResult`, `IntoOutcome`, `ExternalVerb`, `ArgSchema`, `ArgSpec`, `command`, `group` (fn), `external`, `exit_code_for`. Delete `Verb` (trait), `BoxedVerb`, `Exit`, `Group`.

Rewrite `examples/demo.rs` and `tests/cli.rs` to the macro form, keeping every existing assertion (help at three levels, completion, globals placement, suggestion matrix, externals never spawned). Add `verb.path()` assertions for a nested command and for two `create`s in different groups.

## Step 7 — publish mechanics

`RELEASING.md`: publish `verbandi-macros` first. `cargo publish --dry-run` for both crates.

## `docs/PLAN.md` edits

1. "The framework API" code block — replace with the step 1–3 signatures. Keep `pub use bpaf` and the `App::new` "a parser, not an `OptionParser`" comment.
2. "Consumer-side subcommand" — replace with the `#[verb]` function and its two-item expansion, plus `group!` / `app!`.
3. "Tree by composition" bullet — a group is a `Command` with children; a verb's `path()` is assigned during the fold.
4. "bpaf capability check" bullet 3 — `.map(|a| Verb::new(path, move |ctx| run(a, ctx)))`.
5. Constraint 1 — codes are `u8`, `run` returns `ExitCode`.
6. "Testability" — L1 is an argument struct's parser, tested once per shared struct; add the body layer.
7. "v1 scope (subtractive)" — in: `Success`/`Failure`, `Verb`, `Command`/`App`, `#[verb]`, `group!`/`commands!`/`app!`, `cli!`/`verb_test!`, gap-closer. Deferred: `inventory`, currying, argument-level proc-macro sugar.
8. "Verification" — `trybuild` UI tests; macro-expansion equivalence; `verbandi-macros` published first; `Success`/`Failure` codes.
9. "Risks & tradeoffs" first bullet — replace with: one documented function plus its name in one list; residual cost is a proc-macro dependency and errors reported through `bpaf_derive`.
10. Context header — "No bespoke proc-macro for v1" superseded; point at ADR-004.
11. `ROADMAP.md` — add "Function-first verbs (ADR-004)" to the current milestone, ahead of the moniker migration.
