# Roadmap

## Current milestone

Land the first adopter: migrate `moniker`'s CLI onto hicli, now targeting the
post-ADR-007 shape (`#[command]`/`#[group]`/`commands!`/`app!`) —
functions untouched, groups as modules, no list anywhere in the tree. The
remaining question is whether that model survives contact with a real CLI:
replace moniker's `Cli`/enums/`dispatch()` match, keep `bases/cli` parser-free,
and prove exit-code and `.feature` journey parity. `docs/sketches/moniker.rs`
sketches the target shape. Group-level curried arguments (ADR-005's "Future
work", `#[group(args = T)]`) were earned by mdma-cli (#1) and shipped in
0.2.0, see "Releases for mdma-cli" below. Full design is in
`docs/PLAN.md`.

## Releases for mdma-cli

The mdma-cli evaluation (2026-11-03) filed issues #1–#18 against hicli as a clap
replacement. The essentials are split across three releases:

- **0.1.0:** help metadata and examples (#2, #9: ADR-008), completion scripts
  (#12: ADR-009), values starting with `-` (#5; since 0.4 the glued `--added=-7` only, #29), and the
  release itself (#3).
- **0.2.0:** group-level arguments (#1) and a group's default command (#4),
  designed together (ADR-011: they reshape the group node), plus `?` on
  application errors (#6, ADR-012).
- **0.3.0:** what the user sees in help and errors: usage lines name the
  binary with no trailing space (#21, #28, ADR-013), a group's bare default and
  a curried value on group help pages (#19, #20), and the missing-command
  error after a curried value (#27). The promo site ships with it.
- **0.4.0:** coloured help and errors through bpaf's `print_message`, with
  `Error: ` on parse errors and command failures (#31, ADR-015); only the glued
  `--added=-7` for values starting with `-` (#29); long usage lines wrap
  between items (#30, ADR-013 §4, a workaround for bpaf#471).
- **Next:** `mk_ctx` seeing the selected command (#7), pending mdma-cli's
  review of the proposed answer: a lazy context (`ctx.node()?`) and no new API
  (ADR-014 once accepted). Parse errors naming the failed option (#26) need a
  bpaf change and are on hold.
- **Unscheduled:** choice enums (#8), owned command names (#10), the
  positionals-last check (#11), globals-aware completion (#13), and the
  nice-to-haves #14–#18.

## Backlog

<!-- Upcoming work, roughly prioritised. Refine against docs/PLAN.md as it lands. -->

- First adopter: migrate moniker's CLI onto hicli
- Second/third adopters: mdma-cli, ctx
- Verify `#[group] pub mod x;` under rust-analyzer (manual editor check) — the
  ADR-010 spike confirmed `rustc`'s behaviour only
- Deferred until a consumer needs it: richer `ArgSchema` for externals, opt-in
  `--dump-command-manifest` handshake, per-binary hash pin, auto-registration
  sugar
- Deferred until a consumer's pain names it (ADR-006): a field-composing
  `#[derive(Arg)]` for composite types, completion masks on the shell
  shorthands (`complete = files("*.toml")`), globals-aware completion

## Done

<!-- Completed milestones -->

- 0.4.0, colour and wrapping (ADR-015, 2026-11-05, #29 #30 #31): a default
  `color` feature, `Error: ` on parse errors and command failures, long usage
  lines wrapped between items
- 0.3.0, help and errors (ADR-013, 2026-11-05, #19 #20 #21 #27 #28):
  `App::bin_name`, hicli-rendered usage lines, bare defaults and curried values
  on group help pages, a named missing curried command; plus the promo site
- `?` on application errors (ADR-012, 2026-11-04, #6): a command returns its
  own `Result<_, E>` for any `E: Into<Failure>` (std errors, anyhow, eyre,
  `String`), the source chain rendered `outer: mid: root`
- Curried groups and a group's default command (ADR-011, 2026-11-04, #1, #4):
  `#[group(args = T)]`, `#[group(curried = T)]`, `#[command(curried)]`,
  `#[command(default)]`
- 0.1.0 released and published to crates.io (2026-11-03): help metadata and
  examples footer (ADR-008), completion scripts (ADR-009), file groups
  (ADR-010)

- File groups are `#[group] pub mod x;` (ADR-010, 2026-11-03): `group_mod!`
  removed before 0.1.0, MSRV 1.99
- Project scaffolding (`cargo init --lib`, devenv toolchain, docs skeleton)
- Core command model proven (2026-09-11): a trait for the parsed command, plus
  `Exit`/`App`/then-`Group`, composed a `bpaf` command tree with no central
  dispatch `match`; exit codes remapped (usage 2, help 0, runtime 1). Superseded
  by ADR-004 (below): `Group` as a separate builder type is gone, and the parsed
  node — now `Invocation<Ctx>` (ADR-007) — is a struct, not a trait.
- In-process testability: `App::parse`/`into_parser`, `cli!` and `cli_test!`
  macros, L1/L2 test matrix in `tests/cli.rs`
- Git-style external subcommands (`ExternalCommand`), allowlisted by absolute
  path, no `$PATH` scan
- Typo suggestions: `bpaf` built-ins plus a Levenshtein gap-closer over the
  command registry
- Publish readiness baseline: metadata, dual license, `cargo publish --dry-run`
  green
- v1 hardening (2026-09-11): mandatory help text on every then-`leaf`/`group`
  (now every `Command`), in-process external-subcommand tests (nothing spawned),
  suggestion scoped to the command position over the top-level registry, an
  in-process completion test, and accurate "deltas from clap" docs
- `Exit::Code` (2026-09-11): a command — notably an external one — carries a
  specific exit code back as a value; `std::process::exit` is gone from the
  library, `Exit::message()` is `Option<&str>`, and signal deaths report
  `128 + N` on Unix (ADR-003). `Exit` itself is superseded by ADR-004's
  `Success`/`Failure` split (below); the "code is a value, never `process::exit`"
  substance carries forward unchanged.
- `App::run_with_args` run-level test seam (2026-09-11): `App::run` delegates to
  it, so the last public method without coverage — exit-code remap end to end,
  the suggestion gap-closer path, `mk_ctx` failure, dispatch and
  `Exit::Code` propagation — is asserted in-process. Unblocks moniker's
  exit-code parity check.
- **Function-first commands (2026-09-16, ADR-004):** a subcommand is a documented
  function, not a struct implementing a trait plus a registration line. Landed
  in five steps: (1) `Success`/`Failure`/`IntoOutcome` (`src/outcome.rs`)
  replace `Exit`, closing the `Code(0)`→`1` wart structurally — a success code
  now lives on the success side; (2) the parsed command (`Invocation<Ctx>` since
  ADR-007) becomes a struct, not a trait, carrying `name()`/`path()`/`run()`, and
  `Command<Ctx>` unifies what used to be `Group`/leaf into one node type, built
  via `command`/`group`/`external` (`src/command.rs`, `src/invocation.rs`);
  (3) a new exact-pinned `hicli-macros`
  proc-macro crate ships `#[command]`, turning one documented function into a
  `Command` handle plus its body module, reading the ident for the name, the
  doc comment for mandatory help (ADR-001, now enforced by the compiler), and
  each owned parameter's type for its `#[derive(Bpaf)]` constructor; (4)
  `group!`/`commands!`/`app!` declarative macros (`src/macros.rs`) declare the
  tree — a child is a child because it appears in its parent's list, no
  registration call; (5) a `trybuild` UI suite covers `#[command]`'s compile-time
  rejections (missing doc comment, two context parameters, `&mut` context, a
  `self` receiver, generics/lifetimes, non-path/pattern parameters). `App::run`
  now returns `std::process::ExitCode`; `run_with_args`/`exit_code_for` stay
  `u8`. See `docs/adr/004-function-first-verbs.md`.
- **Commands are the function; groups are modules (2026-09-17, ADR-005):** the
  list-form `group! { ctx: Ctx; event: [a, b] }` and the `mod create { run }`
  expansion are gone. Landed in three steps: (1) `#[command]` leaves the annotated
  function verbatim and adds a hidden sibling handle, `__hicli_<ident>()`,
  always `pub` and `#[doc(hidden)]`; `commands!` becomes a proc macro that
  rewrites a bare path's last segment into a handle call (`create` →
  `__hicli_create()`, `a::b` → `a::__hicli_b()`), anything else passing
  through `IntoCommand::into_command`; `app!` forwards its list as `$($c:tt)*`
  so the bare-path distinction survives the forward; (2) `#[group]` on an
  inline module discovers its `#[command]`/`#[group]` children by source order —
  a child is a child because it is in the module — derives its `Ctx` from
  `#[group(ctx = T)]` or the first `&T` among the children, and rejects a
  missing/blank doc comment and an empty group at compile time
  (`macros/src/{shared,discover,ctx,group}.rs`; `tests/groups.rs`, 16 cases);
  (3) `group_mod!` gives file modules the same group, reading the declaring
  file's own path (`Span::local_file()`, MSRV 1.88) and resolving `mod x;`
  declarations against the Reference's mod-rs/non-mod-rs rules
  (`macros/src/{fs,resolve}.rs`), then emitting `#[path]` so rustc reads
  exactly the bytes the macro read (`tests/group_files.rs` + fixtures, 11
  cases). `src/macro_support.rs` and `__group_fn!` are deleted; the dash rule
  and the doc-comment split now exist once, in the proc-macro crate. 22
  `trybuild` cases across both attributes and `group_mod!`. Named `group_mod!`
  rather than `group!` because a `#[proc_macro]` and a
  `#[proc_macro_attribute]` cannot share a name in one crate (confirmed by
  spike); it becomes an alias for `#[group] pub mod x;` once the toolchain pin
  reaches Rust 1.99. See `docs/adr/005-verbs-are-the-function.md`.
- **An argument is a type (2026-09-23, ADR-006):** the snake_case-constructor
  rule moves out of `#[command]` and into `#[derive(Arg)]`, where it becomes one
  line of one impl instead of a call synthesised at every command. `src/arg.rs`
  ships `Arg` (`fn parser() -> impl Parser<Self>`, with
  `#[diagnostic::on_unimplemented]` notes — hicli ships no impls, so a
  primitive in a command's signature is a compile error that teaches the newtype
  pattern), `Candidate` (`= (String, Option<String>)`), and
  `complete_with`/`complete_files`/`complete_dirs`. `macros/src/arg.rs` adds
  `#[derive(Arg)]`, spanned at the type's ident, with
  `#[arg(complete = fn | files | dirs)]` attaching a completer to the type
  once so every command taking it inherits completion for free; a completer on a
  multi-field struct or an enum is a compile error (it cannot tell which field
  is mid-word). `#[command]`'s expansion binds `<T as Arg>::parser()` to a local
  per owned parameter before `construct!`, rather than a synthesised name
  call. `App::complete(args) -> Vec<String>` is the completion-level test
  seam, pinned at bpaf's protocol revision 1 — revision 2 through `run_inner`
  terminates the process, a hazard confirmed by spike and outside hicli's
  control. Verified against bpaf 0.9.28: bpaf passes a completer's output
  through unfiltered (the completer filters), and a completer only fires for a
  value that already parses from a partial word (a `u64` newtype never sees
  the empty word). See `docs/adr/006-an-argument-is-a-type.md`.
- **hicli, and `#[command]` (2026-09-23, ADR-007):** the crate is renamed
  `verbandi` → `hicli` (`hicli-macros`; both names verified free on crates.io),
  and the vocabulary named after the old crate with it: `#[verb]` → `#[command]`,
  `Verb<Ctx>` → `Invocation<Ctx>`, `VerbResult` → `Outcome`, `ExternalVerb` →
  `ExternalCommand`, `verb_test!` → `cli_test!`, handle prefix `__hicli_`. No
  behaviour changes; every recorded diagnostic re-blessed and read; prose drops
  "verb" outside ADR-001…006. `VISION.md` refined the same day: "CLI-enable your
  code", hi-fi → hi-cli. Outside the commit, still the owner's: the GitHub repo
  rename (the `repository` URLs already point at `johlrogge/hicli`), the local
  directory, and moving `~/.metadev/projects/verbandi/` → `hicli/` to match
  `METADEV_PROJECT`. See `docs/adr/007-hicli-and-the-command-attribute.md`.
