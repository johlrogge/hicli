# verbandi

A modular, publishable subcommand framework for Rust CLIs, built on
[`bpaf`](https://docs.rs/bpaf).

> **Status:** v1 is implemented and covered by an in-process test suite. The crate is
> not yet published to crates.io, and the API may still break before the first release.

## The idea

Most `clap`-based CLIs end up less modular than they could be: one subcommand should
be one self-contained unit — its arguments and its behavior — but in practice you get
a derive-enum plus a hand-written central `match` that grows without bound.

`verbandi` makes the alternative the path of least resistance: **one subcommand = one
`Verb`** — a type with annotated arguments and a `run` method — composed into a
command tree by combination, not by a hand-maintained dispatcher.

```rust
pub trait Verb<Ctx> {
    fn name(&self) -> &'static str;
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit>;
}
```

See [`docs/PLAN.md`](docs/PLAN.md) for the full API, the design rationale, and the
`bpaf` capability analysis that grounds it.

## Why `bpaf`

`verbandi` is built on and **re-exports** [`bpaf`](https://docs.rs/bpaf) (parser-as-value
combinators), so consumers pin exactly one `bpaf` version through `verbandi`. `bpaf`'s
parsers are plain values with no process side effects, which is what makes
first-class in-process testing possible (see below).

## Design pillars

- **First-class in-process testability.** Drive parsers with an arg slice — no
  subprocess spawning. Two macros make tests read like a terminal session:
  ```rust
  let (_g, verb) = app().parse(cli! { event create x "My Event" }).unwrap();
  assert_eq!(verb.name(), "create");
  ```
- **Git-style multi-binary subcommands** with shared help and completion — an
  external binary can be a subcommand (`app foo` execs the binary registered for `foo`,
  by absolute path; the git-style `app-foo` naming is a convention, not the contract)
  while `app --help` and completion still describe it, without ever executing it to
  find out. Because
  the manifest is the single source for both the parser and the help text, the test
  suite proves "nothing is spawned" by registering an external at a path that cannot
  exist: help, completion and dispatch all still work. The child's exit code comes
  back as a value (`Exit::Code`), so `verbandi` never exits the process behind your
  `main`'s back; on Unix a signal death is reported as `128 + N`, like a shell.
- **Typo / "did you mean?" suggestions.** `bpaf` supplies suggestions for misspelled
  commands and flags; `verbandi` adds a Levenshtein gap-closer over its top-level
  command registry for the case where `bpaf` stays silent.
- **Trust-conscious external-subcommand discovery** — no implicit `$PATH` scanning,
  ever; external subcommands are allowlisted by absolute path.

## Known deltas from clap

1. **Help text is not byte-identical.** `verbandi` renders `bpaf`'s layout, not `clap`'s.
   The target is semantic equivalence — every command and option documented with the
   same information. If you are migrating a `clap` CLI, assert on substrings of help
   output, not on whole snapshots.
2. **Globals live at the top level, not per subcommand.** `verbandi` composes
   `construct!(globals, verbs)`, so a global flag is accepted both before and after the
   subcommand: `app --home x status`, `app status --home x` and even
   `app event create dir NAME --home x` all parse with `--home` reaching the globals
   (`bpaf`'s positional parsers stop at a dashed token, so a free-form positional does
   not swallow it). The real difference from `clap`'s `global = true` is structural:
   there is no per-subcommand redefinition or propagation — globals are parsed once and
   handed to `mk_ctx`; a subcommand cannot declare, shadow or override them. The one
   exception is an **external** subcommand: everything after its name is forwarded to
   the child verbatim, so a global must *precede* the external's name (`app --home x foo`,
   never `app foo --home x`).
3. **A `--` after an external's name is consumed by `bpaf`** and not forwarded to the
   child. If the child needs a literal `--` in its argv, it has to be passed some other
   way.

## Who uses this

The author's own CLIs, in order: [moniker](https://gitlab.johlrogge.rs/) first (its
handlers are already parser-free), then `mdma-cli`, then `ctx`.

## Getting started (development)

This project runs in an immutable Nix/devenv environment. See `devenv.nix` for the
full toolchain (stable Rust, pinned).

```sh
devenv shell        # or: direnv allow, if you use direnv
just --list          # see all available tasks
just ci               # fmt-check + clippy + test — what CI runs
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
