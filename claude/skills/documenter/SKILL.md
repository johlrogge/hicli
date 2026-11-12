# hicli documentation targets

hicli has more user-facing documentation than the README. Keep all of it in step with the
code; a page that describes last release's API is a bug.

| Target | What it is | Notes |
|---|---|---|
| `README.md` | The full user guide | Version banner at the top ("X.Y.Z, pre-1.0"); quick start `hicli = "X.Y"`; "Upgrading from …" section per breaking release; the first line points at `QUICKREF.md` with its line count — update the count when the file changes |
| `QUICKREF.md` | Short reference for agents and busy humans | Also rendered as rustdoc `hicli::quick_reference` (`src/lib.rs`, `include_str!`), so every `rust` block is a doctest and must compile; clap comparisons are `text`/`rust,ignore`. Keep it dense: one pattern per section, a minimal snippet, a one-line clap equivalent |
| `CHANGELOG.md` | Keep a Changelog | `[Unreleased]` during development; dated section plus compare link at release |
| `docs/PLAN.md` | Design and scope | Scope paragraphs follow the ADRs |
| `site/` (`index.html`, `style.css`) | The promo site, deployed by `.github/workflows/pages.yml` whenever `site/**` changes on `master` | See below |

## The promo site is part of every release

The site describes the **released** version, so it is updated while cutting the release —
on the release branch, before it is finished — never left for later. For each release:

- **Version mentions** — cover, `Cargo.toml` snippet (`hicli = "X.Y"`), fineprint: all say
  the new version.
- **Dependency snippet** — matches what the README's quick start needs (since 0.7.0 no
  `bpaf` line: globals are spelled or composed `Arg` types; `bpaf` only for guards, `some`,
  a fallback shown in help).
- **New features get shown**, not just listed: each user-visible mechanism of the release
  gets a section or example in the site's existing style (chapters, side-by-side `.pair`
  screens, an exercise), and sections describing changed or removed behaviour are rewritten.
- **Output on the page is exact** — help pages and errors are copied from test assertions
  (`tests/*.rs`, `examples/playr.rs`), never written from memory. Say so in your report
  where a sample is derived (e.g. a `bin_name` added to a test's usage line).
- **Links** — docs.rs, QUICKREF, CHANGELOG still resolve.
- It works at phone width (the `.pair` layout stacks below 52rem).

## Sources of truth

Never invent API. Read the ADRs in `docs/adr/` for the release, `README.md`, the examples
(`examples/playr.rs`, `examples/playr_minimal.rs`) and the tests. You cannot run doctests or
`just ci`; list every snippet you are unsure compiles so the orchestrator can verify it.
