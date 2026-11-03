# ADR-008: Command metadata and help layout — the doc comment's three slots

## Status

Accepted (2026-11-03)

Closes johlrogge/hicli#2 (aliases, hidden commands, about/version) and
johlrogge/hicli#9 (examples in `--help`). Extends ADR-001 (help stays
mandatory and lives in the doc comment), ADR-002 (the suggestion registry gains
a second list, its scope is unchanged) and ADR-005 (`#[command]` and `#[group]`
gain keys, read by one shared rule). ADR-004's rejection of `help =` sugar and
of a structure builder **stands**.

Extended by [ADR-011](011-curried-commands-and-default-commands.md) (2026-11-03):
`#[command]` gains the flag keys `curried` and `default`, and `#[group]` gains
`args = T` and `curried = T`. Unlike `alias` and `hide`, these are about a
command's *position in the tree*, not how it is typed or advertised.

## Decision

### 1. All help text lives in the doc comment, in three slots

```rust
/// search the library                       <- help: the listing line and the heading
///
/// Matches are printed newest first.        <- details: bpaf's header, reflowed prose
///
/// # Examples                               <- footer: verbatim, after the options
///
/// ```sh
/// mdma search --artist Bowie
/// mdma search --added -7 --genre jazz
/// ```
#[command(alias = "s")]
pub fn search(q: Query, ctx: &Ctx) -> Outcome { … }
```

- **Help** is the first line, exactly as before.
- **Details** are the prose after it, read exactly as before (each line
  trimmed, leading blanks dropped, joined) — but they now **stop** at the
  examples heading.
- **Footer** starts at the first line after the help whose trimmed text is
  exactly `# Examples` or `Examples:` and runs to the end. `## Examples` is not
  the heading. The body keeps its lines: code fences are dropped, leading and
  trailing blank lines are dropped, the common indentation is removed and each
  non-blank line is indented by two; interior blank lines stay. It is printed
  as `Examples:\n` + the body, however the heading was spelled, through
  `Command::footer` — **after** the options list, verbatim, in plain `--help`
  and `--help --help` alike. A heading with nothing under it is a compile
  error.
- **Fence examples as ```` ```sh ````.** In a library crate rustdoc compiles an
  untagged fence, a ```` ```rust ```` fence and an indented code block as
  doctests — on private items too — and a shell command line is not Rust.
- There is no `footer =`, `about =` or `usage =` key.

### 2. Attribute keys hold only a command's name and its visibility

| Key | On | Means | Primitive |
|---|---|---|---|
| `name = "…"` | `#[command]` only | the canonical spelling | `hicli::command(name, …)` |
| `alias = "…"`, repeatable | `#[command]`, `#[group]` | another spelling, unlisted | `Command::alias` → `bpaf`'s `ParseCommand::long` |
| `hide` | `#[command]`, `#[group]` | out of help, completion and suggestions; still parses | `Command::hide` → `bpaf`'s `Parser::hide` |

The same key means the same thing on both attributes, read by one parser in
`macros/src/shared.rs`. An alias is always mounted with `long`, never `short`
(bpaf matches a one-character command by `long` too). `Invocation::path` stays
canonical, so a test or a `match` on the path never learns which spelling was
typed. Compile errors: an unknown key; `name`, `ctx` or `hide` twice; a name or
alias that is empty, contains whitespace or starts with `-`; an alias that
repeats another alias or equals the command's own name.

### 3. The suggestion registry: every spelling is known, only names are suggested

ADR-002's registry splits in two. **Known** is every spelling of every
top-level command — names and aliases, hidden or not — and an exact match on
any of them gets no hint. **Listed** is the canonical names of the non-hidden
commands, and is the only list the Levenshtein pass suggests from. A hint
never reveals what `--help` does not show. Two siblings sharing a spelling (one
command's alias is the other's name, say) **panic when the tree is folded**,
worded like the empty-group panic: neither function can see the other, so the
macro cannot refuse it, and a shadowed spelling must not be silent.

### 4. `App::about` / `App::version` on the composition root

`App::about(text)` is `bpaf`'s `descr` on the top-level parser: printed above
`Usage:`. `App::version(v)` is `bpaf`'s `version`: `--version` and `-V` print
`Version: v` to stdout and exit `0` (the remap already maps a `Stdout` failure
to `0`). No `app!` keys — `app! { … }.about(…).version(env!("CARGO_PKG_VERSION"))`
reads fine. Once a version is set, **`-V` and `--version` are reserved**: a
global option with either spelling silently shadows bpaf's flag instead of
failing, so it is documented rather than detected.

These two setters are **not** ADR-004's rejected structure builder. That
builder (`Group::new(…).leaf(…).group(…)`) spelled the *tree* — which commands
exist and where — as method calls. `about` and `version` describe the app
itself, which has no item and no doc comment to carry them; the tree is still
declared by modules and functions.

### 5. hicli encodes the footer to a `bpaf` `Doc` itself

`bpaf` has no preformatted block. Its text is prose: lines are reflowed, `\n\n`
is a paragraph break and **plain `--help` prints only the first paragraph** of
a header or footer, and a line indented by four or more spaces becomes a
"code line" whose indent is stripped and whose first line is glued to the text
before it (`Examples:mdma search …`). So `crate::help::verbatim` builds the
`Doc` line by line: each line is its own inline block (`doc.doc(&Doc::from(line))`),
joined by `"\n "`, bpaf's hard line break. A separate block per line stops bpaf
from merging the break with a four-space line into a code line; a blank line is
*nothing* between two breaks, never `"\n\n"`. Verified against bpaf 0.9.27 in
both renderings.

## Why

- **Help text belongs where a human reads it.** ADR-001 and ADR-004 put the
  help in the doc comment so it could not drift from the code; examples are
  help, and the doc comment is where a Rust author already writes
  `# Examples`. A key would be a second place.
- **The footer slot is the only one that works.** The examples spike
  (`tests/help_examples.rs`, before this ADR) showed every spelling broken in
  the details: an indented block reflowed into one line, a heading cut at the
  first paragraph by plain `--help`, a fence printed as words. After the
  options, verbatim, is also where `git`, `cargo` and `clap`'s
  `after_help` put examples.
- **Keys that are not help.** An alias and visibility are about how a command
  is *typed* and whether it is *advertised* — neither is prose, so neither has
  a slot in the doc comment, and both mean the same thing on a group as on a
  command.
- **A hidden command is still a command.** Hiding is about discovery, not
  access: a typed-exactly hidden command runs, so it must count as known for
  the gap-closer, or `app secret` would be answered with a hint about
  `status`.

## Alternatives considered

- **A `footer = "…"` (or `examples = "…"`) key.** Rejected: it is the `help =`
  sugar ADR-004 refused, moving help text out of the doc comment into a string
  literal no editor renders as Markdown.
- **`#[doc(hidden)]` meaning `hide`.** Rejected: `#[doc(hidden)]` is about
  rustdoc, and a command an author hides from API docs is not thereby hidden
  from its users — or the reverse. Two audiences, two switches.
- **bpaf's `docgen` feature.** Rejected: it renders Markdown-ish docs for man
  pages and READMEs, not `--help`, and pulls in a dependency for a layout hicli
  needs a dozen lines for.
- **Untrimmed details** (keep the indentation and let bpaf's code lines carry
  the examples). Rejected: bpaf glues the first code line to the preceding
  text and strips exactly four spaces, and plain `--help` still stops at the
  first blank line.
- **Visible aliases** (`list, ls` in the command list). Deferred: no consumer
  has asked, and bpaf would need the help line rewritten by hand.

## Consequences

- **Deferred, until a consumer's pain names them:** a usage-line override,
  deprecated-name warnings (an alias that prints a hint), visible aliases, an
  author line, an app-level footer.
- **`Command` gains `footer`, `alias` and `hide`**, usable on a leaf, a group
  and an external alike; the `#[command]` and `#[group]` reference expansions
  emit them in a fixed order after `.details(..)`: `.footer(..)`,
  `.alias(..)` per alias in source order, `.hide()`.
- **Footer lines over 100 columns wrap**, and the continuation starts at
  column zero — bpaf's width, not hicli's. Documented, not fixed.
- **New `trybuild` cases** for every new rejection; `tests/help_examples.rs`
  becomes the regression suite for the three examples spellings, and
  `tests/metadata.rs` covers alias, hide, about and version in-process.
- **An existing doc comment containing a line that is exactly `Examples:` or
  `# Examples` changes rendering**: everything from that line on moves from the
  details to the footer. That is the point, but it is a visible change for a
  consumer upgrading.
