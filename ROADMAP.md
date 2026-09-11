# Roadmap

## Current milestone

Prove the core "Verb" model compiles and composes: a self-contained subcommand
type (annotated arguments + `run`) that assembles into a `bpaf`-backed command
tree without a hand-written dispatch `match`. Full design is in
`docs/PLAN.md`.

## Backlog

<!-- Upcoming work, roughly prioritised. Refine against docs/PLAN.md as it lands. -->

- Core `Verb` trait/macro and command-tree composition
- In-process testability: drive parsers with an arg slice, no subprocess
- Git-style multi-binary subcommand discovery with shared help + completion
- Typo / "did you mean?" suggestions on par with clap
- Trust-conscious external-subcommand discovery: no implicit `$PATH` scan,
  allowlist by absolute path only
- crates.io publish readiness (docs, examples, semver policy)
- First adopter: migrate moniker's CLI onto verbandi
- Second/third adopters: mdma-cli, ctx

## Done

<!-- Completed milestones -->

- Project scaffolding (`cargo init --lib`, devenv toolchain, docs skeleton)
