# ADR-011: Curried commands and a group's default command

## Status

Accepted (2026-11-03)

Closes johlrogge/hicli#1 (group-level arguments) and johlrogge/hicli#4 (a
group's default command). Realises the "Future work: currying" of ADR-004 and
ADR-005. Extends ADR-008's attribute keys with `curried` and `default`.

**Amended by [ADR-016](016-hicli-spells-one-value-argument-types.md)
(2026-11-06): a default's doc comment is one paragraph,** not one line — its
lines up to the first blank line, joined with a space, are the help that
titles its options in the group's `--help` (or is shown under `Usage:`). Its
"first doc line" below now reads "first paragraph". Details, an examples
section or an `# Arguments` section on a default is still a compile error;
overrides on a default are deferred.

## Context

mdma-cli needs two shapes a tree of leaves and groups cannot spell:

- `mdma source <NAME> sync|…` beside `mdma source list` — a group that parses a
  value after its name and hands it to some of its commands, while other
  commands take none.
- `mdma search [QUERY] --artist …` beside `mdma search fact-values-for <TYPE>`
  — a group with a command that runs when no sibling is named.

Until now a node was a leaf XOR a group, and siblings could not share a
spelling. `bpaf` is applicative: one part cannot depend on another's parsed
value, so the group's value cannot be passed into a child's parser. It reaches
the child by composing functions *after* parsing.

## Decision

### 1. Currying is explicit and type-checked

```rust
/// manage sources
#[group(args = SourceName)]
pub mod source {
    use super::*;

    /// list sources — a fixed sibling: takes no source name
    #[command]
    pub fn list(ctx: &Ctx) -> Outcome { … }

    /// sync a source
    #[command(curried)]
    pub fn sync(s: SourceName, apply: Apply, ctx: &Ctx) -> Outcome { … }

    /// jobs of a source
    #[group(curried = SourceName)]
    pub mod job {
        use super::*;

        /// cancel a job
        #[command(curried)]
        pub fn cancel(s: SourceName, id: JobId, ctx: &Ctx) -> Outcome { … }
    }
}
```

- **`#[command(curried)]`** curries the command over its **first owned
  parameter**. Its handle returns `hicli::Curried<T, Ctx>`; its parser parses
  only the remaining parameters and yields a function awaiting `T`. The
  function stays untouched and callable as declared (ADR-005).
- **`#[group(args = T)]`** parses one `T` — `<T as Arg>::parser()`, completion
  included — after the group's name. Curried children are collected into a
  `Vec<Curried<T, Ctx>>`, so a first parameter of another type is a compile
  error at that child. Unmarked children are **fixed siblings**, expanded
  exactly as before and matched first. The group composes the parsed `T` with
  each partial function.
- **`#[group(curried = T)]`** is a plain group sitting after an enclosing
  value: it parses nothing, and every child is curried over `T`
  (`source bandcamp job cancel 7`).
- **A word spelling a fixed sibling — its name or an alias, hidden or not — is
  that sibling, never `T`.** A hidden reserved-word guard enforces it: bpaf's
  longest-match alternative would otherwise read `source list sync` as the
  value `list` followed by `sync`.

### 2. A group's default command

```rust
/// search the library
#[group]
pub mod search {
    use super::*;

    /// list values for a fact type
    #[command]
    pub fn fact_values_for(t: FactType, ctx: &Ctx) -> Outcome { … }

    /// search by text
    #[command(default)]
    pub fn query(f: Filters, q: Query, ctx: &Ctx) -> Outcome { … }
}
```

`#[command(default)]` runs when no sibling is named. Trying the named siblings
first suffices for parsing. It **answers to no word** of its own, and its
`Invocation::path` is the group's (`["search"]`). Its options are rendered in
`--help` under a section titled by its first doc line. `search -- fact-values-for`
searches for that literal word. bpaf's positionals-last rule applies: a
default's positionals must come last. bpaf panics when `--help` renders, in any
build, if they do not; hicli renders `--help` once at fold time in debug builds,
so the mistake surfaces on the first debug run or test (see #11). A group holds at most one default; a
default takes no alias, `hide`, details or footer.

**Amended (0.3.0, #19).** A default *without options* was invisible in
`--help`: bpaf keeps a section only when it starts with an item, so its title —
the default's help line — was never printed, and bpaf's `Meta::or` drops an
alternative with no items, so the usage read `COMMAND ...` as if a command were
required. The default's section is now wrapped in a `with_group_help` probe
that records whether it has items. When it has none, the group's header is the
default's help line (then a blank line and the group's details, if any — the
line comes first because plain `--help` prints only a header's first paragraph,
ADR-008), and the usage brackets the derived part: `Usage: playr library
archive [COMMAND ...]`. A default with options is unchanged.

### 3. Runtime: one node type, generic over what it parses to

`Command<Ctx>` becomes an alias for `Node<Invocation<Ctx>>`, and
`Curried<T, Ctx>` for `Node<Partial<T, Invocation<Ctx>>>`. `Node<O>` is the one
type; `group` is generic over `O`. New primitives: `curried_command` (a leaf
over `T`), `group_args` (a group that parses a `T` and hands it to `Curried`
children; it mirrors `#[group(args = T)]`, and "curried" only ever means "receives
`T`") and `Node::by_default`. Source-compatible with 0.1.0.

### 4. Help and completion

- A bound command's usage shows the placeholder: `source SOURCE-NAME sync`.
- `source --help` has a `Commands for SOURCE-NAME:` section for the curried
  commands; fixed siblings stay under `Available commands`.
- Completing a non-empty word where the value goes offers the fixed siblings
  whose names start with it, beside the value's own candidates (`source l` →
  `list`, `search f` → `fact-values-for`).
- A per-mount shared cell carries the first remaining word to the value's
  completer, for completion only; parsed values never travel through it.

**Amended (0.3.0, #20).** The `Commands for SOURCE-NAME:` title was applied to
the value and the bound commands together, so a value with a field doc was
listed as a row of that section, as if it were a command. The title now covers
the bound commands only; the value is listed with the positional items, under
`Available positional items:` (or under its type's doc heading, if the type has
one).

**Amended (0.3.0, #27).** A value with no bound command after it was blamed
itself: both alternatives failed with bpaf's `Missing`, merged into
``expected `COMMAND ...`, got `bandcamp` ``. The bound commands now sit beside a
`pure_with` alternative that fails with hicli's own message, which wins over
`Missing` at the same depth and loses to a bound command that matched and
failed deeper (`source bandcamp cancel` still reports its missing `ID`). The
peeked words carry the first undashed word after the value, so the message
either suggests the closest bound command (``no such command after
`bandcamp`: `snyc`, did you mean `sync`?``, the same Levenshtein rule as
ADR-002's top-level suggestion) or lists the listed ones (``expected a command
after `bandcamp`: `sync`, `status` or `cancel` ``). A lone word that is a typo
of a fixed sibling (`source lst`) keeps bpaf's `did you mean `list``. The
alternative's meta is `Skip`, so usage and help are unchanged.

## Known limits (bpaf 0.9.27)

- `source <TAB>` with nothing typed also offers the curried commands, which are
  not valid until a value is typed; picking one leads to a confusing error.
- `source NAME --help` shows the group's page, not the command's.
- A misspelled sibling under a default reads as the default's query.
- zsh/bash may be offered the metavar as the sole candidate when nothing
  matches — bpaf's normal positional behaviour.
- A one-match value completer hides a prefix-matching fixed sibling
  (`source l` → only `local`): bpaf drops the metavariable item when an inner
  completer returns exactly one suggestion. A future non-breaking fix could
  expose the type's completer via a defaulted `Arg` method.
- The old name-last order `source sync bandcamp` reads `sync` as the value
  (``expected a command after `sync`: …``). Reserving bound names at the value
  position is unsound, since `sync` is a legal value.

## Deferred

- Nested `args` groups (an `args` group inside a curried slot): it parses at
  runtime, but has no constructor or macro form.
- A default inside a curried group.
- A top-level default.
- A default that also answers to its own name. Recipe: a second `#[command]`
  calling the same function.
- A dedicated `source NAME --help` page.
- Suggestions inside curried groups, beyond the command right after the value
  (covered since 0.3.0, #27).

## Alternatives considered

- **Implicit currying by parameter type, with `TypeId` slots.** Rejected:
  runtime type erasure, and every command pays for a feature most do not use.
- **First parameter by convention, without a mark.** Rejected: it needs textual
  type matching in the macro, and an alias or a re-export would silently turn a
  curried command into a fixed one.
- **Mounting the same command twice** (once fixed, once curried). Rejected:
  two spellings of one function, and the tree states membership twice.
- **The group macro matching types by tokens.** Rejected: a type is not its
  spelling; the compiler must do the checking, which `Vec<Curried<T, Ctx>>`
  makes it do.
- **Passing the parsed value through a shared cell, or rewriting argv**, so a
  child can read the group's value. Rejected: hidden state between parsers, and
  untestable at the parse layer. (The completion-only cell in §4 carries a raw
  word, never a parsed value.)
- **`#[group(default = f)]`.** Rejected: names a child in a list, which
  ADR-005 removed.
- **A default function named like its group.** Rejected: it forces the group
  and its default to share an ident and breaks the module/handle naming.
- **A named default** (`search query …` also works). Rejected for now: see
  Deferred; the recipe covers it.

## Consequences

- Type errors for a misplaced curried command name `Curried` or
  `Node<Partial<…>>`.
- Curried help, usage and completion are partly composed by hicli rather than
  rendered by bpaf. `tests/bpaf_contract.rs` pins bpaf's behaviour
  byte-for-byte and **must be re-run on every bpaf upgrade**.
- The completion layer depends on bpaf internals: `construct!` evaluating left
  to right, `ParseComp` calling the completer eagerly inside eval, and one
  evaluation per parse. The unit tests and `tests/bpaf_contract.rs` pin them.
  `bpaf = "0.9.27"` is a caret requirement, so consumers may resolve a newer
  0.9.x the contract test never saw. A future CI job should run
  `cargo update -p bpaf && just test`; the repo has no CI yet.
- New tests: `tests/curried.rs`, `tests/default_command.rs` and new `trybuild`
  cases for each rejection.
