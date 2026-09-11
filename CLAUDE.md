# verbandi

A standalone, crates.io-publishable Rust framework for building modular CLIs: one
subcommand = one self-contained "Verb" (a type with annotated args + a `run`),
composed into a command tree instead of a central hand-written dispatch `match`.
Built on and re-exports [`bpaf`](https://docs.rs/bpaf).

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
verbandi targets stable-Rust consumers (moniker, mdma-cli, ctx).

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
- **`bpaf` is re-exported, not wrapped opaquely.** Consumers pin one `bpaf` version
  through `verbandi`; don't hide `bpaf` types behind newtypes without a reason spelled
  out in `docs/PLAN.md`.
- **Testability is first-class and in-process.** No subprocess spawning in tests —
  drive parsers with an arg slice via `App::parse` / the `cli!` / `verb_test!` macros.
- **Subtractive scope.** `docs/PLAN.md`'s "v1 scope (subtractive)" section is
  authoritative — don't add auto-registration macros, proc-macro derives, or other
  conveniences until a real consumer's pain justifies them.
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
  `.claude/skills/architect/SKILL.md` (project-provided — not yet written; see
  `.claude/skills/rust-architect/references/` for the underlying reference docs on
  async/tokio, error handling, lifetimes, patterns, testing, tooling, type-driven
  design, polylith, ECS, and embedded — load on-demand, do not read wholesale)
- **code-minion** — implements what the architect specifies
- **commit** — writes commits per `.claude/skills/conventional-commits/SKILL.md`
- **helix** skill — keymap design expert (not directly relevant to a CLI-parsing
  crate, but installed as part of the standard metadev skill set)

## Verification

Before calling a change done: `just ci` green, plus whatever `docs/PLAN.md`'s
"Verification" section calls for at that stage (parser/dispatch/suggestion golden
matrix, `cargo publish --dry-run`, `examples/` CLI exercised end-to-end).
