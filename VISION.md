# Vision

> Draft — captured from the decisions and intent behind `verbandi`. Refine freely;
> this is the north star, not a spec. The *what* and *how* live in `README.md`,
> `ROADMAP.md`, and `docs/PLAN.md`.

## The belief

Pretty much any `clap` CLI ends up less modular than it could be. The derive-macro
path pulls you toward a central enum of subcommands and one hand-written dispatch
`match` that grows without bound — the arguments for a command live in one place, its
behavior in another, and adding a command means editing a shared file that everyone
else is also editing.

It doesn't have to be that way. **A subcommand is naturally one self-contained thing:
its arguments *and* its behavior, together, testable on its own.** `verbandi` exists to
make that the path of least resistance — one subcommand = one `Verb`, composed into a
tree by combination, so the central dispatcher disappears.

## What we're really buying

Not fewer lines of code — **locality and testability**. Adding a command becomes a new
file and one registration line, not a scar across a monolith. A command can be
understood, changed, and tested in isolation. On a large CLI (the kind that grows to
thousands of lines in a single file) that difference compounds.

## The bets that make it worth a crate

`verbandi` is opinionated about four things `clap` treats as secondary or leaves to you:

1. **Testing is first-class, in-process.** You drive a parser with the arguments a user
   would type and assert on the result — no subprocess, no brittle string-scraping.
   Tests read like a terminal session (`cli! { event create x "My Event" }`), so the
   test *is* the documentation of what the user types.
2. **A CLI can span several binaries and still feel like one** — the `git` model.
   `app foo` can hand off to `app-foo`, yet `app --help` and shell completion still
   describe `foo`, without executing anything to find out.
3. **Good error ergonomics are not optional.** Typo/"did you mean?" suggestions stay on
   par with `clap`; a misspelled command or flag points you at the right one.
4. **Discovery is trust-conscious by construction.** No implicit `$PATH` scanning, ever;
   external subcommands are allowlisted by absolute path and never handed secrets
   implicitly. A framework used by security-sensitive tools must not widen their attack
   surface as a side effect of being convenient.

Built on `bpaf` (parsers as plain values), which is what makes 1 and 2 honest rather
than aspirational.

## What success looks like

- **It proves itself on real CLIs.** `moniker` migrates onto it first (its handlers are
  already parser-free), with identical behavior, exit codes, and passing journey tests —
  then `mdma-cli` and `ctx`. Adoption by the author's own tools is the acceptance test.
- **It's a good crates.io citizen** — documented, dual-licensed, semver-disciplined,
  small enough to read in one sitting.
- **The modular path is genuinely easier** than the central-match path it replaces. If
  writing a new command ever feels heavier than a `match` arm, the design has failed.

## Non-goals (at least for now)

- **Not a `clap` replacement for everyone.** It's for people who feel the modularity
  itch, not a claim that everyone should switch.
- **Not a proc-macro DSL.** v1 leans on `bpaf`'s own derive plus small declarative
  macros. Auto-registration sugar (`inventory`-style) waits until a real consumer's pain
  justifies it — earn the mechanism, don't front-load it.
- **Not a kitchen sink.** Every feature must pull its weight against the subtractive
  default: improve the core idea and remove, rather than pile on mechanisms.

## Guiding principle

Subtractive design. The win is a *smaller, clearer* way to build a CLI. Prefer sharpening
the `Verb` concept and deleting scaffolding over adding knobs. When in doubt, leave it
out until a consumer proves it's needed.
