# hicli

**CLI-enable your code.** A standalone, crates.io-publishable Rust framework for
building modular CLIs: one subcommand = one documented function (`#[command]`),
groups are annotated modules (`#[group]`, `group_mod!` for file modules until
Rust 1.99), arguments are types (`#[derive(Bpaf, Arg)]`, completion attached to
the type), composed into a command tree with `app!` instead of a central
hand-written dispatch `match`. Built on [`bpaf`](https://docs.rs/bpaf).

The crate and its vocabulary were renamed by
[ADR-007](docs/adr/007-hicli-and-the-command-attribute.md), which is the
dictionary between the older ADRs' words and today's names.

The full design — API surface, rationale, scope, verification plan — lives in
[`docs/PLAN.md`](docs/PLAN.md). Read that before making architectural changes; this
file covers conventions and day-to-day workflow, not design.

Roadmap and current milestone: [`ROADMAP.md`](ROADMAP.md).

## Environment

This project runs in an immutable Nix environment managed by devenv.
**Do NOT** run `pip install`, `npm install -g`, `cargo install`, `brew install`,
`apt-get install`, or any other imperative package manager.
If a tool or package is missing, add it to `devenv.nix` and re-enter the shell.
All tools, packages, hooks, and services are declared in `devenv.nix`.

The Rust toolchain is stable (pinned in `devenv.nix`, currently 1.97.0) with
`rustc`, `cargo`, `clippy`, `rustfmt`, and `rust-analyzer`. No nightly features —
hicli targets stable-Rust consumers (moniker, mdma-cli, ctx).

## Conventions

Task runner: `just` (see `justfile` for the full list; `just --list` to enumerate).

| Command | What it does |
|---|---|
| `just build` | `cargo build --workspace --all-targets` |
| `just test` | `cargo test --workspace` |
| `just clippy` | `cargo clippy --workspace --all-targets -- -D warnings` |
| `just fmt` | `cargo fmt --all` |
| `just fmt-check` | `cargo fmt --all -- --check` |
| `just doc` | `cargo doc --no-deps` |
| `just ci` | `fmt-check` + `clippy` + `test` — mirrors what CI runs |
| `just clean` | `cargo clean` |

Run `just ci` before considering any change done.

### Crate conventions (see `docs/PLAN.md` for the why)

- **Stable Rust only.** No nightly-only features, ever — this is a dependency other
  stable-toolchain CLIs pin.
- **`bpaf` is re-exported, not wrapped opaquely.** Don't hide `bpaf` types behind
  newtypes without a reason spelled out in `docs/PLAN.md`. Consumers using
  `#[derive(Bpaf)]` for their argument structs also add `bpaf = "0.9"` directly
  (the derive expands to `::bpaf::…` paths that don't resolve through a
  re-export) — hicli states the required version, cargo unifies it; see
  ADR-004.
- **Testability is first-class and in-process.** No subprocess spawning in tests —
  drive parsers with an arg slice via `App::parse` / the `cli!` / `cli_test!` macros.
- **An argument is a type.** hicli ships no `Arg` impls — a primitive in a
  command's signature (`fn create(name: String, ctx: &Ctx)`) is a compile error by
  design, teaching the newtype pattern at the point it is broken; see ADR-006.
- **Subtractive scope.** `docs/PLAN.md`'s "v1 scope (subtractive)" section is
  authoritative. Earned mechanisms (ADR-004, ADR-005, ADR-006): `#[command]`
  (leaves the function untouched, adds a handle, calls `<T as Arg>::parser()` per
  owned param), `#[group]` (an annotated module, children discovered by source
  order — no list), `group_mod!` (the same group for a file module, until Rust
  1.99), `commands!` (a bare path names a handle), `Arg` + `#[derive(Arg)]` +
  `#[arg(complete = …)]`/`files`/`dirs` (an argument is a type that parses
  itself, and completion attaches to that type once and is inherited by every
  command that takes it) — because moniker's pain justified each — and the
  `alias`/`hide` keys, the doc comment's `# Examples` footer plus
  `App::about`/`App::version` (ADR-008), and `completion_script` (ADR-009),
  because mdma-cli's migration from clap justified them. Deferred until
  the next real consumer's pain justifies them: `#[group] pub mod x;` as the
  direct spelling (blocked on the toolchain pin reaching Rust 1.99.0),
  group-level currying (`#[group(args = T)]`), a field-composing
  `#[derive(Arg)]` for composites, `#[arg(parser = …)]`, completion masks,
  globals-aware completion, `inventory`-style auto-registration (rejected
  outright, not just deferred).
- **No implicit `$PATH` scanning** for external subcommand discovery — allowlist by
  absolute path only. This is a trust boundary, not a convenience default.

## Workflow

For any non-trivial code change, follow the multi-agent workflow:

1. **architect** — review and design; never writes code
2. **code-minion** — implements based on architect's instructions
3. **You (orchestrator)** — delegate; do NOT implement directly

When you reach for Edit or Write on source files: stop. Spawn a code-minion instead.
Small, isolated, obviously-safe changes (config values, typos) may be done directly.
Everything else goes through the workflow.

## Tool Usage Policy

**Always prefer MCP tools over Bash.** This project inherits MCP servers from
metadev (`git-read`, `git-write`, `gh-issues`, `gh-ci`, `gh-repo`, `rust-codebase`,
`just`, `devenv`, `cargo-polylith`, `adr`, `ssh`, `mcp-test`) — plus any
project-specific servers declared in its `devenv.nix`. Use them first.

When no MCP covers the operation:
1. State in one sentence why you're using Bash (e.g. "no MCP tool for `git ls-remote`").
2. Consider whether the gap is worth a feature request against metadev
   (`gh_issue_create` with `repo: "johlrogge/metadev"`, `label: "enhancement"`) or the
   project's own MCP server.

When an MCP tool exists but misbehaves:
- **Do not fall back to Bash as a workaround.** File a bug and/or fix the root cause.
  A silent fallback hides the defect.

## Agents & Skills

- **architect** — reviews/designs Rust architecture; loads
  `.claude/skills/architect/SKILL.md` (project context: the command model, crate layout,
  design contracts, delegation workflow). The underlying Rust reference docs live in
  `.claude/skills/rust-architect/references/` (async/tokio, error handling, lifetimes,
  patterns, testing, tooling, type-driven design, polylith, ECS, embedded) — load
  on-demand, do not read wholesale
- **code-minion** — implements what the architect specifies
- **commit** — writes commits per `.claude/skills/conventional-commits/SKILL.md`
- **helix** skill — keymap design expert (not directly relevant to a CLI-parsing
  crate, but installed as part of the standard metadev skill set)

## Verification

Before calling a change done: `just ci` green, plus whatever `docs/PLAN.md`'s
"Verification" section calls for at that stage (parser/dispatch/suggestion golden
matrix, `cargo publish --dry-run`, `examples/` CLI exercised end-to-end).
