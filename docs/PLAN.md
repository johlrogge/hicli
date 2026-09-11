# `verbandi` — a modular, publishable subcommand framework

> This is the approved design/plan for `verbandi`. It doubles as the working reference
> while the crate is built. Status: scaffolding + v1 skeleton.

## Context

**Why this exists.** The hypothesis: *pretty much any clap CLI ends up less modular than it
could be.* Ideally one subcommand = one function/type with annotated arguments, its parser and
dispatch **derived by composition**, not hand-written into a central `match`. A scan of the
author's projects confirms the gap — every clap CLI uses derive enums + a hand-written dispatch,
from clean (`cargo-polylith`) to monolithic (`mdma-cli`, 7292 lines in one file; moniker's
~330-line `dispatch()` match; `ctx`).

**Decisions:**
- **Deliverable:** a **standalone, crates.io-publishable framework crate `verbandi`**, its own
  repo, a metadev project with its own devenv.
- **Consumers:** the author's CLIs adopt it as an external dependency — **moniker first** (its
  handlers are already parser-free), then `mdma-cli` (biggest payoff) and `ctx`. Consumer
  migrations are follow-up work.
- **Mechanism:** built on **bpaf** (parser-as-value combinator style); `verbandi` re-exports
  `bpaf` so consumers pin one version. No bespoke proc-macro for v1.
- **Must not regress from clap:** first-class **testability**, **git-style multi-binary
  subcommands with shared help + completion**, and **typo/"did you mean?" suggestions**.

## bpaf capability check (grounds the design)

- **Subcommands are values:** `OptionParser<T>::command(name)` yields a `Parser<T>` to store/compose.
- **Command owns its parser next to its handler:** args via `#[derive(Bpaf)]` or `construct!`.
- **"Run this command" polymorphism is a `.map()`:** `OptionParser<Args>.map(|a| Box::new(a) as BoxedVerb)`, combined via `construct!([a,b,c])` / `choice(...)`. No central match.

**Three constraints `verbandi` handles for consumers:**
1. **Exit-code remap.** bpaf: parse error → 1, help/version → 0. Consumers often want usage → 2.
   `verbandi` calls `run_inner`, intercepts `ParseFailure`, and **remaps** (never
   `OptionParser::run()`, which auto-exits with bpaf's codes).
2. **Secrets-off-argv is a consumer concern.** bpaf's `env()` shows in `--help` and is settable on
   argv. `verbandi` keeps context-building in a consumer-supplied `mk_ctx` closure so things like
   `MONIKER_PASSPHRASE` stay hand-read env vars, never fields.
3. **Globals don't float like clap `global = true`.** `app --home x sub …` is natural; trailing
   `… --home x` is stricter in bpaf. Documented as the one expected behavior delta.

## The framework API

```rust
pub use bpaf; // consumers pin ONE bpaf version through us

/// One subcommand = one Verb. `Ctx` = the consumer's shared context.
pub trait Verb<Ctx> {
    fn name(&self) -> &'static str;                     // for tests, help, suggestions
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit>;
}
pub type BoxedVerb<Ctx> = Box<dyn Verb<Ctx>>;

/// Consumer exit-code convention, decoupled from bpaf's.
pub enum Exit { Usage(String), Runtime(String) }        // code(): 2 / 1
impl Exit { pub fn runtime<E: Display>(e: E) -> Self; }

/// Composition builder.
pub struct App<G, Ctx> { /* global parser + Vec<Parser<BoxedVerb<Ctx>>> + registry */ }
impl<G: 'static, Ctx> App<G, Ctx> {
    pub fn new(globals: bpaf::OptionParser<G>) -> Self;
    pub fn leaf(self, name: &'static str, p: impl Parser<BoxedVerb<Ctx>> + 'static) -> Self;
    pub fn group(self, name: &'static str, sub: Group<Ctx>) -> Self;
    pub fn external(self, ext: ExternalVerb) -> Self;   // git-style plugin
    pub fn run(self, mk_ctx: impl FnOnce(G) -> Result<Ctx, Exit>) -> i32;

    // --- test seams (in-process, no subprocess) ---
    pub fn into_parser(self) -> bpaf::OptionParser<(G, BoxedVerb<Ctx>)>;
    pub fn parse(self, args: &[&str]) -> Result<(G, BoxedVerb<Ctx>), bpaf::ParseFailure>;
}
pub fn exit_code_for(f: &bpaf::ParseFailure) -> i32;    // Stderr => 2 (usage), else 0
```

- **Tree by composition:** leaves combine via `construct!([...])`; a group is
  `construct!([inner…]).to_options().command(name)` boxed back to `BoxedVerb<Ctx>`.
- **Globals + context:** top-level parser yields `(G, BoxedVerb<Ctx>)`; `App::run` hands `G` to
  `mk_ctx`, then `verb.run(&ctx)`. Maps `ParseFailure` → 2/0 and `Exit` → 2/1.

**Consumer-side subcommand (the pattern this enables):**
```rust
#[derive(Bpaf, Clone)]
pub struct Create {
    #[bpaf(positional("EVENT_DIR"))] event_dir: String,
    #[bpaf(positional("NAME"), some("a name is required"))] name: Vec<String>,
}
impl Verb<Ctx> for Create {
    fn name(&self) -> &'static str { "create" }
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit> { /* call domain handler + print */ }
}
pub fn create_cmd() -> impl Parser<verbandi::BoxedVerb<Ctx>> {
    create().map(|c| Box::new(c) as verbandi::BoxedVerb<Ctx>)   // bpaf-derived `create()`
}
```

## Testability (first-class, in-process)

bpaf's edge over clap: parsers are plain values; `run_inner(Args::from(&[...]))` returns
`Result<T, ParseFailure>` with **no process side effects** — `ParseFailure` exposes rendered text
(`unwrap_stderr()`/`unwrap_stdout()`) and `exit_code()`. `verbandi` surfaces this via
`App::parse` / `into_parser` / `exit_code_for`.

**Three layers:** L1 per-command parser (assert fields); L2 App dispatch + exit intent (selected
`verb.name()`, remapped exit code, rendered stderr); L3 consumer domain handlers (unchanged).

### Terminal-shaped test macros (the tests read like what the user types)

A signature ergonomic goal: a test should show, verbatim, **the command line the user types**,
mapped to a test function. `verbandi` ships two macros:

- **`cli! { … }`** — takes the command line as **raw tokens** (no wrapping string), so it reads
  exactly like typing in a terminal, and expands to the arg vector for `App::parse`:
  ```rust
  let (_g, verb) = app().parse(&cli! { event create x "My Event" }).unwrap();
  assert_eq!(verb.name(), "create");
  ```
  Each bare token is `stringify!`- d; a **string-literal token** is taken by value — that's how an
  argument containing spaces/special chars is written. Tokenizer caveat (documented on the macro):
  a single-quoted multi-word arg (`'My Event'`) does not lex in Rust, so a spaced argument MUST be
  a double-quoted literal `"My Event"`; single-token args need no quotes (`event create x`).
- **`verb_test! { … }`** — generates a `#[test]` per case; each case's left side is the typed
  command-line tokens (same form as `cli!`), parsed via a consumer-provided `app()` builder, and
  the `Result<(G, BoxedVerb<Ctx>), ParseFailure>` is handed to the closure. The tokens *are* the
  test's documentation:
  ```rust
  verb_test! {
      app: crate::app();
      create_ok: { event create x "My Event" } => |r| {
          let (_g, verb) = r.expect("parses");
          assert_eq!(verb.name(), "create");
      },
      missing_name_usage: { event create x } => |r| {
          let e = r.unwrap_err();
          assert_eq!(verbandi::exit_code_for(&e), 2);
          assert!(e.unwrap_stderr().contains("NAME"));
      },
  }
  ```
  (Exact `app()`-supply convention — a leading `app: <expr>;` header vs per-case — finalized during
  implementation; keep it minimal.)

These macros are the crate's headline testing UX and must be covered by the crate's own tests +
shown in the example CLI.

## Multi-binary subcommands with shared help + completion (the git model)

`app foo ARGS…` exec's external `app-foo`, yet `app --help` and completion still enumerate `foo`.
clap's `allow_external_subcommands` dispatches but the subtree is opaque. `verbandi` carries **one
manifest both the parser and help/completion read**:
```rust
pub struct ExternalVerb { name: &'static str, help: String, bin: PathBuf, schema: Option<ArgSchema> }
```
An external becomes a bpaf `command(name)` whose parser collects the remainder verbatim
(`any::<String>(...).many()`) and `.map`s into a `Verb` that runs
`Command::new(ext.bin).args(rest).status()`, propagating the child's exit code.

- **Advertising the subtree — static allowlist manifest is the authority**, with an optional,
  opt-in runtime handshake (`app-foo --dump-verb-manifest`) only to enrich completion for
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
1. **Golden matrix** in tests (via `cli!`/`verb_test!` + `unwrap_stderr()`): misspelled
   top-level/nested commands, long flags, short/long confusion — assert stderr names the target.
2. **Gap-closer using the same registry:** on `Err(ParseFailure::Stderr)` with no suggestion, run
   a Levenshtein pass (≤ 2; `strsim`) over registered names/flags and append
   `` did you mean `<name>`? ``. One registry feeds help, completion, and suggestions.

## v1 scope (subtractive)

`Verb` + `Exit` + `App` (leaf/group/external/run + test seams) + `cli!`/`verb_test!` macros +
suggestion gap-closer. **Defer** `macro_rules!`/`inventory` auto-registration sugar until a real
consumer's pain justifies it.

## Verification

- `devenv shell`: build, test (parser/dispatch/suggestion golden matrix + macro tests, all
  in-process), `clippy -D warnings`, `fmt --check`.
- `cargo publish --dry-run` succeeds (metadata + license complete).
- `examples/` CLI runs: good parse, usage error (exit 2 after remap), a typo (suggestion),
  `--help` at top + group level.
- **Follow-up (separate work):** adopt `verbandi` in moniker (replace `Cli`/enums/`dispatch()` in
  `projects/moniker/src/main.rs`; keep `bases/cli` parser-free; prove exit-code + `.feature`
  journey parity), then mdma-cli, ctx.

## Risks & tradeoffs (honest)

- **More modular, or relocated glue?** The central match disappears; each command becomes
  colocated + independently testable; adding one = a new file + one `.leaf(...)` line. But you
  still write struct + `Verb` impl + registration per command. **Win = locality + testability,
  not less total code.**
- **bpaf vs clap UX — two known deltas:** help text not byte-identical (target semantic
  equivalence); trailing global options stricter than `global = true`. Document in README.
- **crates.io name:** verify `verbandi` is free at publish time before first `cargo publish`.
