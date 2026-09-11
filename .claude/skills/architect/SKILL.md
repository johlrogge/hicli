---
name: architect
description: verbandi project context for the architect — the Verb model, crate layout, design contracts, and delegation workflow.
---

# verbandi — architect context

`verbandi` is a standalone, crates.io-publishable Rust **library crate** (not Polylith,
not a workspace) that provides a modular CLI framework on top of `bpaf`. The north star
is in `VISION.md`; the full API design, rationale, and `bpaf` capability analysis are in
`docs/PLAN.md` — **read `docs/PLAN.md` before reviewing any non-trivial change.**

## The core idea (what every review defends)

One subcommand = one `Verb` (a type carrying its annotated arguments + a `run`), composed
into a command tree by combination — **no central hand-written dispatch `match`.** If a
change reintroduces a growing central dispatcher, or makes adding a command heavier than
"a new file + one registration line," that is a design regression worth calling out.

## Crate layout

- `src/verb.rs` — `Verb<Ctx>` trait, `BoxedVerb<Ctx>`, `Exit` (Usage/Runtime → codes 2/1).
- `src/app.rs` — `App<G, Ctx>` composition builder (`leaf`/`group`/`external`/`run`), the
  `into_parser`/`parse` test seams, `exit_code_for`, and the `strsim` suggestion gap-closer.
- `src/external.rs` — `ExternalVerb`/`ArgSchema`/`ArgSpec` (git-style multi-binary model).
- `src/testing.rs` — arg-vector helpers backing the macros.
- `src/macros.rs` — `cli! { … }` (raw-token command line) and `verb_test! { … }`.
- `examples/demo.rs`, `tests/cli.rs` — living docs + the in-process test matrix.

## Design contracts to hold the line on

- **`bpaf` is re-exported** (`pub use bpaf`) so consumers pin exactly one version through
  `verbandi`. Don't let a second parser dependency leak into the public API.
- **Exit codes are remapped, not `bpaf`'s defaults.** `App::run` must go through
  `run_inner` + `exit_code_for` (usage → 2, help/version → 0, runtime → 1). Never call
  `OptionParser::run()` (it auto-exits with `bpaf`'s codes).
- **Secrets never become parser fields.** Context (incl. anything secret) is built in the
  consumer's `mk_ctx` closure from the environment, not from `bpaf` `env()` args.
- **Testability is in-process.** Parser/dispatch behavior is asserted via `App::parse` and
  the macros — no subprocess. Tests should read like a terminal session.
- **External-subcommand discovery is trust-conscious.** No implicit `$PATH` scan;
  allowlist by absolute path; no implicit secret forwarding to child binaries.

## Guiding principle

**Subtractive design.** The value is a *smaller, clearer* way to build a CLI. Prefer
sharpening the `Verb` concept and deleting scaffolding over adding knobs. Defer mechanism
(e.g. `inventory`-style auto-registration, a proc-macro DSL) until a real consumer's pain
justifies it — earn it, don't front-load it. API surface should be minimal and hard to
misuse.

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
