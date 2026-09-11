# Roadmap

## Current milestone

Land the first adopter: migrate `moniker`'s CLI onto verbandi. v1 is hardened
(see Done) — the remaining question is whether the `Verb` model survives contact
with a real CLI: replace moniker's `Cli`/enums/`dispatch()` match, keep
`bases/cli` parser-free, and prove exit-code and `.feature` journey parity. Full
design is in `docs/PLAN.md`.

## Backlog

<!-- Upcoming work, roughly prioritised. Refine against docs/PLAN.md as it lands. -->

- `Exit::Code(i32)` so `ExternalRun::run` can propagate a child's exit code
  without calling `std::process::exit` inside the library (skips destructors,
  makes that verb un-testable in-process). Close before moniker adopts externals.
- First adopter: migrate moniker's CLI onto verbandi
- crates.io publish: verify the `verbandi` name is free, tag `0.1.0` per
  `RELEASING.md`
- Second/third adopters: mdma-cli, ctx
- Deferred until a consumer needs it: richer `ArgSchema` for externals, opt-in
  `--dump-verb-manifest` handshake, per-binary hash pin, auto-registration sugar

## Done

<!-- Completed milestones -->

- Project scaffolding (`cargo init --lib`, devenv toolchain, docs skeleton)
- Core `Verb` model proven (2026-09-11): `Verb`/`Exit`/`App`/`Group` compose a
  `bpaf` command tree with no central dispatch `match`; exit codes remapped
  (usage 2, help 0, runtime 1)
- In-process testability: `App::parse`/`into_parser`, `cli!` and `verb_test!`
  macros, L1/L2 test matrix in `tests/cli.rs`
- Git-style external subcommands (`ExternalVerb`), allowlisted by absolute path,
  no `$PATH` scan
- Typo suggestions: `bpaf` built-ins plus a Levenshtein gap-closer over the
  command registry
- Publish readiness baseline: metadata, dual license, `cargo publish --dry-run`
  green
- v1 hardening (2026-09-11): mandatory help text on every `leaf`/`group`,
  in-process external-verb tests (nothing spawned), suggestion scoped to the
  command position over the top-level registry, an in-process completion test,
  and accurate "deltas from clap" docs
