# verbandi

A modular, publishable subcommand framework for Rust CLIs, built on
[`bpaf`](https://docs.rs/bpaf).

> **Status:** early scaffolding. The API described below and in
> [`docs/PLAN.md`](docs/PLAN.md) is the approved design, not yet fully implemented.
> Expect breaking changes before a `1.0`.

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
  let (_g, verb) = app().parse(&cli! { event create x "My Event" }).unwrap();
  assert_eq!(verb.name(), "create");
  ```
- **Git-style multi-binary subcommands** with shared help and completion — an
  external binary can be a subcommand (`app foo` execs `app-foo`) while `app --help`
  and completion still describe it, without ever executing it to find out.
- **Typo / "did you mean?" suggestions** on par with `clap`.
- **Trust-conscious external-subcommand discovery** — no implicit `$PATH` scanning,
  ever; external subcommands are allowlisted by absolute path.

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
