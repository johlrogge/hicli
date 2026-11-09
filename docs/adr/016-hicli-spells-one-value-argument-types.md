# ADR-016: hicli spells one-value argument types; the help is the first paragraph

## Status

Accepted (2026-11-06)

Closes johlrogge/hicli#8 (choice values that complete) and johlrogge/hicli#23
(a per-command help for a shared argument type). Extends ADR-006: an argument
is still a type that parses itself, and `#[derive(Arg)]` gains a second mode in
which hicli writes that parser. Amends ADR-001 and ADR-008 (the help is the
first paragraph, not the first line), ADR-004 (a hicli attribute dialect is
allowed on a *type*; decision 1 stands), ADR-009 (`Shell` is a `Choice`) and
ADR-011 (a default's doc comment is one paragraph). ADR-014 is reserved for
johlrogge/hicli#7.

Amended by ADR-018 (2026-11-09): a spelled type parses through its own FromStr
first; a struct with several fields that implements FromStr may be spelled; a
parse error names the argument; a Choice's help row lists its names.

## Context

mdma-cli's migration from clap had two pains that 0.4 could not answer:

- **Choice values did not complete.** A choice was an enum with a
  hand-written `FromStr`, wrapped in a `#[derive(Bpaf, Arg)]` newtype. bpaf
  hands a completer the value that **already parsed** (ADR-006 §5), and no
  prefix of `bpm` other than `bpm` parses as a sort key, so `sort b<TAB>`
  offered nothing, and the error text was whatever each `FromStr` said.
- **A shared argument type had one help everywhere.** `SortOpt` reads "Sort
  the listing by this field" in every command, and `list` wanted "Order for
  this listing". ADR-004 decision 1 rules out parameter attributes, and bpaf
  drops a doc comment on an `external` field, so there was nowhere to say it.

Behind both: the help, the completion and the spelling of a one-value type
were all bpaf's, written through `#[derive(Bpaf)]`, and hicli could change
none of them. Most of mdma-cli's argument types are a newtype over one value.

## Decision

### 1. `Choice`: a fieldless enum is a fixed set of words

```rust
/// What to sort by: bpm, title or artist
#[derive(Debug, Clone, Copy, PartialEq, Choice, Arg)]
#[arg(positional = "KEY")]
pub enum SortKey {
    /// Beats per minute
    Bpm,
    /// Track title
    Title,
    /// Artist name
    Artist,
}
```

`trait Choice: FromStr<Err = UnknownChoice> + Display` carries `VARIANTS`,
`name` and `description`. `#[derive(Choice)]` writes all three impls from the
variants: each is spelled as its ident in kebab-case or its
`#[choice(name = "…")]`; matching is exact and case-sensitive; a variant's
description is the first paragraph of its doc comment. `UnknownChoice` lists
the names and suggests one (the same word in another case first, then one or
two edits away), and does not repeat the typed word, because bpaf prints
``couldn't parse `key`: `` in front of it:

```text
couldn't parse `bmp`: expected `bpm`, `title` or `artist`; did you mean `bpm`?
```

`complete_choice::<C>(typed)` is the completer. A choice is a value; it is an
`Arg` only when `#[derive(Arg)]` (decision 2) says where it goes. `Shell` is a
`#[derive(Choice)]`, so `UnknownShell` is a deprecated alias of
`UnknownChoice`.

### 2. Spelled mode: `#[arg(positional | long | short …)]`

```rust
/// Sort the listing by this field
#[derive(Debug, Clone, Arg)]
#[arg(long = "sort", short = 's', metavar = "FIELD")]
pub struct SortOpt(pub Option<SortField>);
```

When `#[arg(…)]` has `positional`, `long` or `short`, `#[derive(Arg)]` writes
the bpaf chain itself. The type is a tuple newtype, a struct with one named
field, or a fieldless enum that derives `Choice`. `Option<T>` is optional,
`Vec<T>` repeats (recognised syntactically, as `bpaf_derive` does), a `bool`
with a name is a switch. The chain parses a `String`, attaches completion,
then converts with `FromStr` — since ADR-018, the type's own `FromStr` when it
has one, else its field's — because bpaf's `complete` runs only when the
inner parser succeeded, completing the word *before* it is validated is what
lets a half-typed word complete. A `PathBuf` or `OsString` with no dynamic
completer stays a typed parser, keeping bpaf's non-UTF-8 conversion.

**The keys are minimal:** `positional` / `positional = "KEY"`, `long` /
`long = "name"`, `short = 'c'`, `metavar = "M"`, and `complete = f | files |
dirs`. A bare `positional` or `long` defaults from the ident (`SortBy` →
`SORT-BY`, `--sort-by`). `complete = f` is `fn(&str) -> Vec<Candidate>` and is
handed the raw word — so it fires on the empty word, even for a `u16`.

**A key is added only when a consumer earns it.** `fallback`, `env`, `hide`,
guards and the rest of bpaf's attribute surface are not mirrored; a type that
needs one stays in bpaf mode. Without any spelling key the derive is exactly
ADR-006's bpaf mode, unchanged, so the change is additive.

Every generated path is `::hicli::bpaf::…`, so a crate whose argument types are
all spelled needs no direct `bpaf` dependency. The derive writes three items:
the snake_case constructor `#[derive(Bpaf)]` would have written (so
`#[bpaf(external(sort_opt))]` in a bpaf-mode struct still finds it), the `Arg`
impl calling it, and an `ArgHelp` impl (decision 5).

Rejected at compile time, each with its own message: no or a blank doc
comment; `positional` with `long`/`short`; a positional `bool`; `metavar` on a
`bool` or with `positional`; an option with a value but no `metavar`;
`Option<bool>`/`Vec<bool>`; `complete` on a `bool`; a bad `short` or an empty
`long`; a duplicated or unknown key; spelling keys on a unit struct or an
enum with data (lifted for a struct with several fields by ADR-018, which
parses it through its own `FromStr`); `#[bpaf(…)]`
attributes on a spelled type; an enum that is not a `Choice` (E0277, the
trait's note). A spelled type that also derives `Bpaf` is E0428 (two
constructors), and a completer of the wrong signature is a type error.

### 3. A `Choice` inside completes automatically: the autoref probe

`SortOpt` above completes `SortField`'s names with no `complete` key. A proc
macro sees tokens, not traits, so it asks the compiler through method
resolution:

```rust
use ::hicli::__private::{IsChoice as _, NotChoice as _};
let __complete = (&::hicli::__private::Probe::<SortField>::NEW).completer();
```

`IsChoice` is implemented for `Probe<T>` where `T: Choice`; `NotChoice` for
`&Probe<T>` for every `T`. On the receiver `&Probe<T>`, lookup finds
`IsChoice` first when it applies and auto-references to `NotChoice`
otherwise, which returns `no_candidates`. The type is concrete at every call,
which is what makes this sound on stable. The probe was proved by a unit test
before anything was built on it (`src/private.rs`); the fallback, had it not
resolved, was an explicit `complete = choices` key.

ADR-018 renames the traits `IsChoice`/`NotChoice` (they were
`ChoiceCompleter`/`NoCompleter`) and adds `help_row`, which appends a choice's
names to the help row.

### 4. A self-spelled `Choice` goes through `__private::choice`

The spec had a self-spelled choice enum (`#[derive(Choice, Arg)]` with
`Self` as the value) call `complete_choice::<Self>` and `w.parse::<Self>()`
inline. As built, its chain ends in one helper instead:

```rust
::hicli::__private::choice::<Self, _>("KEY", help, |help| {
    ::hicli::bpaf::positional::<String>("KEY").help(help)
})
```

`fn choice<C: Choice, P: Parser<String>>(name, help, head: impl FnOnce(Doc) -> P)
-> impl Parser<C>` does both steps under **one** `C: Choice` bound. Since
ADR-018 the call is `choice::<Self, _>(name, help, |help| head)`: it also
writes the help row and names the argument in the error. An enum spelled by `#[arg(…)]` that
forgot `#[derive(Choice)]` is then a single E0277 carrying `Choice`'s own note,
instead of that plus a `FromStr` error listing the standard library's
implementors. Everything in `hicli::__private` is `#[doc(hidden)]`, named by
generated code only, and may change in any release.

### 5. `ArgHelp` and a command's `# Arguments` section

```rust
pub trait ArgHelp: Arg {
    fn parser_with_help(help: &'static str) -> impl Parser<Self>;
}
```

A spelled type implements it; its `Arg::parser` is `parser_with_help` with its
own doc comment. A hand-written `Arg` may implement it. A bpaf-mode type does
not, because bpaf builds its help. It is a separate trait rather than a
defaulted method so an override on a type that cannot honour it is E0277 with
notes we write, not silently ignored.

A command overrides a parameter's row in its doc comment, never on the
parameter:

```rust
/// List tracks
///
/// Prints one track per line.
///
/// # Arguments
///
/// * `by` - Order for this listing
#[command]
fn list(by: SortOpt) { … }
```

The handle then binds `<SortOpt as ::hicli::ArgHelp>::parser_with_help("Order
for this listing")` for `by` and `<T as ::hicli::Arg>::parser()` for the rest;
without the section the expansion is unchanged, and the function is emitted
as written (ADR-005). Only the help row changes — usage, completion and errors
are the type's.

**Grammar.** The section starts at a line that is exactly `# Arguments` or
`Arguments:` after trimming (as `# Examples` / `Examples:` do), and ends at the
examples heading or the end of the comment. The details now end at the first
of the two headings; the section is not part of the details. An `# Arguments`
line *after* the examples heading is examples body, verbatim. An entry is a
bullet line `` * `name` - help `` (or `` - `name` - help ``); lines indented
past the bullet continue it and are joined with spaces, so an entry is one
paragraph; a blank line, the next bullet or the section's end ends it. Blank
lines between entries are allowed.

**Rejections,** each spanned at the doc line: a line that is neither an entry
nor a continuation; a name that is not a parameter (a pattern-bound one
included), with the parameter list; the `&Ctx` parameter ("it is not parsed, so
it has no help"); a curried first parameter ("its group parses it: its help is
the type's"); a name given twice; an empty section; the section on a
`#[group]` ("a group has no parameters"); the section on a
`#[command(default)]`, by ADR-011's one-paragraph rule. An entry for a type
that is not `ArgHelp` is E0277.

### 6. The help is the first paragraph, everywhere

**A user decision, replacing ADR-001/ADR-008's "first line".** The help is the
doc comment's first paragraph: consecutive non-blank lines up to the first
blank line, each trimmed, joined with single spaces. A heading (`# Examples`,
`# Arguments` or their `…:` spellings) ends the paragraph even without a blank
line. One rule, for:

- **commands and groups** — the details start after the first blank line. This
  is breaking: a second line that directly followed the first used to start
  the details and is now part of the help. The fix is a blank `///` line.
- **a `#[command(default)]`** — its doc comment is one paragraph (was one line).
- **spelled argument types** — the help row; everything after the first blank
  line is rustdoc only and never printed. A missing or blank doc comment is a
  compile error.
- **`# Arguments` entries** — one paragraph each, through continuation lines.
- **`Choice` variants** — the completion description.

One rule means a doc comment wraps the way prose wraps, with no type of item
where a second line silently means something else.

### 7. A use-site doc comment is never rendered

A doc comment where an argument type is *used* — a field of a `#[derive(Bpaf)]`
struct embedding it through `#[bpaf(external(sort_opt))]` — is never printed;
bpaf drops it, and hicli does not resurrect it. The row is the type's help,
and the only override is a command's `# Arguments` entry. A help therefore has
exactly two sources, both in the doc comments a reader already reads.

### 8. Deferred: a field-composing derive, where a field's doc is the override

Struct-field overrides are deferred to the field-composing `#[derive(Arg)]`
that ADR-006 §7 already deferred. When a consumer earns it, the design is: a
multi-field struct whose fields are `Arg` types composes their parsers with
`construct!`, and a field's doc comment, when present and the field's type is
`ArgHelp`, is that field's help override — the struct-level counterpart of
`# Arguments`, through the same `parser_with_help`. Until then a composite
hand-writes `Arg::parser()`.

### 9. Direction: outgrowing bpaf

Spelled mode is hicli owning the **derive layer** for the shape most CLIs are
made of — a type with one value — while bpaf stays the runtime: parsing, help
rendering, completion protocol. The direction this sets:

- **Own the derive layer** where a consumer's pain says bpaf's derive is the
  obstacle, one earned key at a time.
- **Take runtime limits upstream** rather than patching around them in hicli.
  The maintainer files upstream issues and pull requests themselves; agents
  only draft them (title, body, reproduction) for the maintainer to file, and
  never post to a repository the maintainer does not own.
  ADR-013 §4's usage-line wrap is a workaround for bpaf#471, kept only until
  bpaf fixes it; an error that names the value but not the option (#26) waits
  on a bpaf issue the same way — for spelled types ADR-018 names the argument
  in the derive layer; types bpaf spells still wait.
- **Revisit replacing the runtime only on real pain** — a limit bpaf will not
  take upstream and a consumer cannot live with. Not before, and not for
  uniformity's sake: the derive layer moving to hicli is not a plan to move
  the rest.

## Why

- **Completion and error text are now the type's, written once.** A choice
  completes with no code, its error names the choices, and a newtype around it
  inherits both — the ADR-006 promise ("declare it once on the type") kept for
  the case that mattered most.
- **The override has a home that is not a parameter attribute.** rustdoc's
  own `# Arguments` convention is where Rust developers already document a
  function's parameters; reading it keeps ADR-004 decision 1 intact and the
  function's signature untouched.
- **The common case loses its boilerplate.** A one-value type was a derive, a
  field doc comment and a `#[bpaf(…)]` with bpaf's call syntax; it is now a doc
  comment on the type and one `#[arg(…)]`, and a crate that only has such types
  drops its `bpaf` dependency.
- **The probe keeps the key set minimal.** Without it, every choice option
  would need `complete = choices`, a key that exists only because a macro
  cannot see traits.

## Alternatives considered

- **Parameter attributes** (`fn list(#[arg(help = "…")] by: SortOpt)`).
  Rejected: ADR-004 decision 1 ruled them out and still does. They put an
  argument's spelling back at the command, and every command that takes the
  type would be a place to forget it.
- **A `help = "…"` key on `#[command]`** naming parameters
  (`#[command(help(by = "…"))]`). Rejected for the same reason ADR-008 kept
  help out of keys: help lives in the doc comment.
- **Mirroring bpaf's attribute surface** (`fallback`, `env`, `guard`, `hide`,
  …) in `#[arg(…)]`. Rejected for now: a parallel dialect is the cost ADR-004
  named, and no consumer has asked. Each is deferred until one does.
- **An explicit `complete = choices` key** instead of the probe. Kept as the
  fallback, not needed.
- **A defaulted `parser_with_help` on `Arg`.** Rejected: a bpaf-mode type would
  silently ignore an override.
- **Rendering a use-site field doc as the override.** Rejected: bpaf drops it,
  and two override sources would need a precedence rule.
- **Keeping "first line" for commands and using "first paragraph" only for new
  items.** Rejected by the user: two rules for the same doc comment shape.
- **Spelling multi-field structs.** Rejected: that is `#[derive(Bpaf)]`'s job,
  and the field-composing derive (decision 8) is its eventual answer.
  Revisited by ADR-018: a struct with several fields that parses itself from
  one word is spelled; composing fields stays deferred (decision 8).
- **Replacing bpaf's runtime.** Not now; see decision 9.

## Consequences

- New public API: `Choice`, `#[derive(Choice)]`, `UnknownChoice`,
  `complete_choice`, `ArgHelp`; `#[arg(positional, long, short, metavar)]`;
  `# Arguments` in a command's doc comment. `hicli::__private` is hidden.
- Breaking: the first-paragraph rule (commands, groups, defaults, `Choice`
  variants); `# Arguments` / `Arguments:` in a command's details now starts a
  section; `Shell`'s `FromStr::Err` and error text; `UnknownShell` is an alias,
  no longer a tuple struct. Listed in the changelog with their fixes.
- `#[derive(Arg)]` in bpaf mode is unchanged, pinned by a snapshot test
  (`tests/arg_derive.rs`), so both forms stay documented and migration is one
  type at a time. `examples/demo.rs` stays in bpaf mode; `examples/playr.rs`
  spells every one-value type.
- The direct `bpaf` dependency is needed only by a crate that uses
  `#[derive(Bpaf)]` (typically for `Globals` or a multi-field struct).
- Spelled mode depends on bpaf facts verified against 0.9.28: `complete` runs
  only on an inner `Ok`; a `.parse` failure keeps completions; `optional` and
  `many` are applied last. A bpaf upgrade that changes them breaks the
  completion tests, not silently.
