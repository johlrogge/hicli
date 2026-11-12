# ADR-019: `#[derive(Arg)]` composes a struct of argument types; `Option<T>` is "T, if given"

## Status

Accepted (2026-11-12)

Extends ADR-006 (an argument is a type, and a struct of argument types is
one; `Option<T>` of an argument type is an argument position, not a new type)
and ADR-016 (§8's deferred field-composing derive; §2 gains `env`). Amends
ADR-016 §7 (a composed struct's field doc comment is rendered), ADR-018 §1 (a
validated newtype becomes optional at its use, not by an `Option` inside it),
ADR-004's consumer footprint, and ADR-006's "`Globals` does not implement
`Arg`". Partly addresses johlrogge/hicli#26 for globals.

## Context

- **Globals** are the last reason a hicli user adds `bpaf = "0.9"`
  (ADR-004): `App::new` takes a parser, written with `#[derive(Bpaf)]`. An
  external user called this "the abstraction leaks". The README's `--node`
  is bpaf-spelled, so its error is not named (#26). playr's `--node` and
  three of mdma-cli's globals use `env(…)`.
- **Composites.** A struct of argument types is a `#[derive(Bpaf)]` struct of
  `#[bpaf(external(T::parser))]` fields, or a hand-written `Arg::parser()`
  (mdma-cli: `RekordboxImportArgs`, `UploadArgs`, `MergeArgs`,
  `ImportPlaylistArgs`, `ExportArgs`, `SearchArgs`). A type shared by two
  composites has one help, so mdma-cli spells `--output OUTPUT` twice to
  give it two.
- **Optional validated values.** ADR-018 parses `T(Option<I>)` through `I`'s
  `FromStr` and never `T`'s, so a newtype that carries its own rule
  (`Limit(NonZeroUsize)`, mdma-cli's `BpmArg`, `KeyArg`, `DurationArg`, …)
  cannot be optional without a second value type. mdma-cli's `SearchArgs`
  makes seven such types optional with `#[bpaf(external(T::parser), optional)]`.

## Decision

### 1. A struct with named fields and no spelling keys is composed

| on the type | mode |
|---|---|
| `positional`, `long` or `short` | spelled: one argument, one word (ADR-016, ADR-018) |
| none; named fields; no `#[bpaf(…)]` anywhere | composed |
| otherwise | bpaf mode (ADR-006), unchanged |

A proc macro cannot see `#[derive(Bpaf)]`, so it reads `#[bpaf(…)]`
attributes instead. The two readings disagree only for a `#[derive(Bpaf, Arg)]`
struct with named fields and no `#[bpaf(…)]`, and that fails with E0428: both
derives write its constructor (as in ADR-016 §2). Spelling keys decide between
one word (ADR-018's `FromStr` struct) and a composition.

The derive composes the fields with `construct!` in declaration order and
writes the snake_case constructor (so `#[bpaf(external(filters))]` and
`app! { globals: globals() }` work) and `Arg`, with `OPTIONAL` set (§3). It
does not write `ArgHelp`. No fields, and `complete`, `metavar` or `env` on a
composed type, are compile errors.

### 2. Every field is an argument type, or `Option` of one

A field is what a command's parameter is (decision 3): a type `T: Arg`, or
`Option<T>`. Its help, completion and errors are its type's. A doc comment on
the field overrides the type's help, its first paragraph going through
`ArgHelp::parser_with_help` (the struct-level counterpart of `# Arguments`,
ADR-016 §5). A type that is not `ArgHelp` is E0277. With no doc comment, the
type's own help applies. `#[arg(…)]` goes on a type, never on a field: on a
field (or an enum variant), in any mode, it is a compile error that says so.
`#[cfg]` and `#[cfg_attr]` on a composed field are refused too: the
composition names every field.

### 3. `Option<T>` is "T, if given"

In a command's parameters and a composed struct's fields, `Option<T>`
(recognised by its last path segment and one type argument, as `bpaf_derive`
does) expands to `T`'s parser, made optional:

    <T as ::hicli::Arg>::parser().optional()

With an `# Arguments` entry or a field doc, the inner call is
`<T as ::hicli::ArgHelp>::parser_with_help("…")`, so the override applies to
the row and the result is still optional. An optional positional is still a
positional and comes last. A `curried` command's first parameter cannot be
`Option`: its group always hands it a value. `Option<Option<T>>` says optional
twice and is refused with that message. `Vec<T>` and `Option` of
anything that is not an argument type stay E0277.

**Everything not marked optional is required, and the derive marks it.**
`Arg` has a defaulted associated const:

    const OPTIONAL: bool = false;

`true` means the type's parser succeeds without its argument, so
`Option<Self>` would always be `Some`. `#[derive(Arg)]` writes
`const OPTIONAL: bool = true;` for a spelled `Option` or `Vec` inside and for
a switch. A composed struct's is the AND of its fields' —
`true && <A as Arg>::OPTIONAL && …`, an `Option<T>` field counting as `true` —
because a composite succeeds with nothing given exactly when every field does.
`Option<EventRef>` over two required positionals is therefore allowed: absent
is `None`, both words are `Some`, and one word alone is an error, not `None`.
The derive writes nothing — required — for a spelled type that reads exactly
one required word (a newtype or one-field struct over a plain value, a struct
parsed whole, a self-spelled `Choice`) and in bpaf mode. A hand-written `Arg`
writes nothing to be required, and `const OPTIONAL: bool = true;` when it is
optional this way.

Beside each `Option<T>` use, at item level, the expansion asserts it:

    const _: () = ::core::assert!(!<T as ::hicli::Arg>::OPTIONAL, "`T` succeeds when its argument is absent, …");

spanned at `T` where it is used, so `fn f(a: Option<Added>)` fails with
E0080 pointing at `Added`. It is an item, not an inline `const {}` in a
generic function, so it is evaluated under `cargo check`. A const item cannot
name an outer type's generic parameters; `#[derive(Arg)]` refuses generic
types, so a composed struct's assertion is always writable.

**What to write.** `Option<T>` at the use, where `T` is required, is the
default form, and it keeps `T`'s own `FromStr` (ADR-018). A type that is
optional at every use (a filter, a free-text query, a global) holds its
`Option` inside: `struct Added(Option<DaysBack>)`.

### 4. The struct's doc comment is a heading

Its first paragraph is the struct's `group_help`, as bpaf's derive renders
it. No doc comment, no heading.

### 5. `env = "VAR"`

A spelled option with a value may name an environment variable to fall back
to: `long("node").env("PLAYR_NODE")`, rendered `[env:PLAYR_NODE: N/A]`,
and parsed and named like the option. It is rejected on a positional, on a
switch, without `long`/`short`, and when empty or containing `=`. A required
option with `env` is still required: under `Option<T>`, a value from the
environment counts as given. An env-sourced error says ``from `$VAR` `` and
never echoes the value: ``couldn't parse: `--node` from `$PLAYR_NODE`: …``.

### 6. Globals

`App::new` takes any parser, and a spelled or composed type's constructor is
one. A single global is a spelled newtype, optional by nature:

    /// The node to talk to
    #[derive(Debug, Clone, Arg)]
    #[arg(long = "node", metavar = "HOST", env = "PLAYR_NODE")]
    pub struct Globals(pub Option<Node>);

Several globals are a composed struct, headed by its doc comment. A single
global has no heading and is listed under "Available options". A fallback is
an `Option` defaulted in `mk_ctx`. `#[derive(Bpaf)]` globals keep working.

## Why

- **Globals stop leaking bpaf**, and `--node` errors name `--node`.
- **One rule for every argument position.** A parameter and a field are both
  an argument type, or `Option` of one.
- **A validated value can be optional without a second type**, and keeps its
  own rule: `Option<Limit>`, `Option<BpmArg>`.
- **Optional means "absent is `None`", checked at compile time** — for a
  composite too: `Option<T>` of one is allowed exactly when it has a required
  field, so that leaving it out can mean `None`.
- **A shared type can read differently where it is used**, through the field
  doc.

## Alternatives considered

- **Spelling a value on a field.** Rejected: a second place where spelling
  lives.
- **Rejecting `Option<T>`.** Rejected: a validated newtype could not be
  optional without a second value type (ADR-018 §1), which is what mdma-cli's
  `SearchArgs` needs seven times.
- **No guard on `Option<T>`.** Rejected: `Option<Added>` would be `Some`
  forever, compiling and silently wrong.
- **A `RequiredArg` marker trait.** Rejected: opt-in inverts the rule. Every
  hand-written required `Arg` would need a line to be usable as `Option<T>`;
  with a defaulted const, everything not marked optional is required.
- **An autoref probe instead of a const.** Rejected: a probe chooses, it
  cannot refuse with a message, and the derive already knows statically.
- **A runtime check (parsing no input when the app is built).** Rejected: we
  keep checks at compile time.
- **An inline `const {}` assertion.** Rejected: inside a generic handle it is
  evaluated only at monomorphisation, which `cargo check` skips.
- **`Option<T>` for a curried parameter or as a group's `args`.** Out of
  scope: a group's value is always there.
- **An explicit `#[arg(compose)]`, composing every no-key struct, a second
  derive.** Rejected (see the decision 1 discussion).
- **Ignoring a field doc on a non-`ArgHelp` type.** Rejected, as in ADR-016.
- **A heading for a single global.** Not needed by any consumer.
- **`fallback`/`default`, `some`, `Vec<T>` of an argument type,
  tuple-struct composition, a composite's heading override.** Deferred.

## Consequences

- New public API: composed `#[derive(Arg)]`, `env`, `Arg::OPTIONAL`.
  `Option<T>` in parameters and fields is additive, since it was E0277.
- Known gap: a hand-written or `#[derive(Bpaf)]` type that is optional but
  does not set `OPTIONAL` is treated as required, so `Option<ThatType>`
  compiles and is always `Some`.
- A generic composite would have to skip the assertion for a field whose inner
  type names a type parameter; today `#[derive(Arg)]` refuses generics, so no
  field is skipped.
- Breaking:
  - a `#[derive(Bpaf, Arg)]` struct with named fields and no `#[bpaf(…)]` (E0428);
  - `#[arg(…)]` on a field;
  - playr's `--help` loses its `Global options` heading;
  - `Arg`'s and `ArgHelp`'s diagnostics are reworded;
  - golden expansions and trybuild snapshots must be re-blessed.
- An undocumented field deliberately keeps its type's help, and an
  undocumented composite has no heading, so a crate that denies `missing_docs`
  allows it on those items.
- Relies on bpaf 0.9.28 behaviour, pinned by tests: `construct!`'s struct
  form, `group_help`, `NamedArg::env`, `.optional()` on a positional and on
  a named argument.
