# ADR-018: A spelled type parses itself, names itself in errors, and lists its choices

## Status

Accepted (2026-11-09)

Amends ADR-016: §2 (which `FromStr` converts the word; a struct with several
fields that implements `FromStr` may be spelled), §3 (the probe also writes the
help row), §4 (`__private::choice` takes the help and the argument's name) and
§9 (for spelled types, #26 is answered in the derive layer). Partly addresses
johlrogge/hicli#26. ADR-014 is johlrogge/hicli#7.

## Context

mdma-cli's `src/args.rs` opens with ~140 lines — an `impl_arg!` macro with
several arms and its own `Choice` trait with `parse_choice`, `choice_names` and
`choice` — that 0.5's spelled mode was meant to replace. Three things stop it:

- **The validation lives on the newtype's own `FromStr`.** mdma's newtypes
  wrap foreign types — `ContentHash`, `PlaylistName`, `NonZeroUsize`,
  `PathBuf` — so the orphan rule puts the rule on the newtype:
  `impl FromStr for LimitArg` says "must be a whole number of at least 1",
  `impl FromStr for ExistingFileArg` checks that the file exists. Spelled mode
  writes `w.parse::<NonZeroUsize>()` and `.map(LimitArg)`: the type's own
  `FromStr` is never called. It compiles, it parses, and `ExistingFileArg`
  accepts every word — silently wrong.
- **A choice's help lists its names by hand.**
  `format!("Field to sort by: {}", choice_names::<Self>().join(", "))` is a
  help computed from the type, which a doc comment cannot do, so mdma keeps a
  hand-written `Arg` for each choice. One such hand-written list already lies:
  it offers formats the type rejects.
- **An error names the value, not the argument (#26).** bpaf prints
  ``couldn't parse `0`: <message>`` and has no slot for the option. ADR-016 §9
  sent that to bpaf. But for a spelled type hicli writes the `.parse` closure
  and knows the spelling at macro time.

## Decision

### 1. A one-value type parses through its own `FromStr` first

| the type | a word is parsed by |
|---|---|
| `T(I)`, `T { f: I }` | `T`'s `FromStr` if `T` has one, else `I`'s, then wrapped |
| `T(Option<I>)`, `T(Vec<I>)` | `I`'s `FromStr`, per word; `T`'s own is never used |
| `T(PathBuf)` / `T(OsString)`, no `complete = f` | `T`'s `FromStr` (the word must be UTF-8), else the path as bpaf read it (non-UTF-8 kept) |
| `T(Option<PathBuf>)` …, no `complete = f` | the path as bpaf read it |
| a struct `T` with several fields | `T`'s `FromStr` (decision 2) |
| a fieldless enum `T` | `T`'s `FromStr`, which `Choice` requires |
| `T(bool)` | a switch: no word |

The outer type wins because it is the more specific one: it is the type the
command asked for. A word inside `Option`/`Vec` is one `I`; the outer type
cannot parse "one word into an optional", so it is not asked.

A proc macro cannot see whether `T: FromStr`, so it asks the compiler through
method resolution, as ADR-016 §3 does for `Choice`:

```rust
(&::hicli::__private::FromStrOf::<LimitArg, NonZeroUsize>(LimitArg)).parse_word(w)
```

`SelfFromStr` is implemented for `FromStrOf<T, I>` where `T: FromStr`;
`FieldFromStr` for `&FromStrOf<T, I>` where `I: FromStr` (it parses an `I` and
applies the wrapping function it carries). Lookup on `&FromStrOf<T, I>` finds
`SelfFromStr` first and auto-references to `FieldFromStr` otherwise. The impls
bound only `FromStr`, never its `Err`, so a type whose `FromStr` exists is
never skipped over in silence. A type where neither applies is E0599, which
names both missing bounds (`T: FromStr`, `I: FromStr`). For a path value with no
`complete = f` the head stays typed (`positional::<PathBuf>`), and a second
pair, `SelfFromOs`/`FieldFromOs`, takes the path: `T`'s `FromStr` on its UTF-8
text, or the path unchanged.

### 2. A struct with several fields may be spelled when it parses itself

```rust
/// Path to file (audio file or ZIP archive)
#[derive(Debug, Clone, Arg)]
#[arg(positional = "FILE", complete = files)]
pub struct UploadFileArg { pub path: PathBuf, pub kind: UploadKind }

impl FromStr for UploadFileArg { … }
```

The keys mean **one argument, one word**. A struct with several fields
under them is parsed whole through its own `FromStr`; without one it is E0277
(`UploadFileArg: FromStr`). This does not collide with ADR-016 §8's deferred
field-composing derive: a composite has no single spelling, so it carries no
`positional`/`long`/`short`. A unit struct and an enum with data are still
rejected.

### 3. A parse error names the argument

hicli writes the closure, so it prefixes the message with the argument's
spelling, in backticks as bpaf writes names:

```text
couldn't parse `0`: `--limit`: must be at least 1
couldn't parse `xyz`: `HASH`: not a content hash
couldn't parse `bmp`: `--sort`: expected `bpm` or `title`; did you mean `bpm`?
```

- An option is named by its long name (`--limit`), or by its short one
  (`-l`) when it has no long; a positional by its metavar (`HASH`). The name is
  the spelling, not what was typed: `-l 0` reports `--limit`.
- It applies to every spelled parse: own or field `FromStr`, each word of an
  `Option`/`Vec`, a self-spelled `Choice`, a struct parsed whole.
  `UnknownChoice` already leaves out the typed word, so the two compose
  without repetition.
- The error type must implement `Display` (bpaf already required `ToString`):
  `String`, `&str`, any `std::error::Error`, `UnknownChoice` and `Infallible` do.
- **What a `FromStr` error says:** what is wrong with the word, as a phrase
  that reads after the argument's name. Not the word — bpaf prints it — and
  not the argument — hicli prints it. `must be at least 1`, not
  `'0' must be a whole number of at least 1`.

Types bpaf spells — fields of a `#[derive(Bpaf)]` struct, `Globals` — and
hand-written `Arg` impls are unchanged. For those, #26 still waits on bpaf
(decision 6).

### 4. A `Choice` lists its names in its help row

```text
    KEY               What to sort by [bpm, title, artist]
    -s, --sort=FIELD  Sort the listing by this field [bpm, title]
```

After the first paragraph, one space, then `[`, the names in `VARIANTS` order
joined by `, `, and `]`. Each name is a bpaf `Doc::literal` (bold in colour,
plain otherwise); the brackets and commas are text. It is on the same row, so
bpaf wraps it with the help. It applies wherever the value is a `Choice`: a
self-spelled enum, a newtype over a choice (found by the probe), `Option<C>`
and `Vec<C>`, and when a command's `# Arguments` entry replaces the prose.
A choice with no variants gets no suffix. A doc comment that listed the names
by hand now shows them twice: drop them from the doc comment.

### 5. What the expansion calls

All of it in `hicli::__private` (hidden, not API):

- `Probe<T>` with `IsChoice`/`NotChoice` (were `ChoiceCompleter`/`NoCompleter`):
  `completer()` as before, and `help_row(help) -> Doc`.
- `FromStrOf<T, I>(fn(I) -> T)` with `SelfFromStr`/`FieldFromStr`
  (`parse_word(&str)`) and `SelfFromOs`/`FieldFromOs` (`parse_os(I)`).
- `named(name, error) -> String`: the decision-3 prefix.
- `choice::<C, _>(name, help, |help| head)`: a self-spelled choice's whole
  chain — help row, completion, parse, name — still under one `C: Choice`
  bound, so a forgotten `#[derive(Choice)]` is still one E0277.

### 6. What remains of #26

A type bpaf spells and a hand-written `Arg` still print
``couldn't parse `pi.local:http`: `http` is not a port number``. Naming those
needs bpaf's `ParseFailed` to carry the failing item's name. Per ADR-016 §9 the
maintainer files that upstream; agents only draft it.

## Why

- **The newtype pattern now works end to end.** ADR-006 makes the argument a
  type; for a foreign value, that type is where the rule must live. Spelled
  mode now honours the rule instead of quietly bypassing it.
- **Errors say which argument failed** without a key, and without consumers
  repeating the spelling in their messages.
- **A choice's help is its type's**, like its completion and its error: written
  once, never out of date.
- **It stays in the derive layer** (ADR-016 §9): hicli changes the code it
  writes, not bpaf's runtime.

## Alternatives considered

- **A key (`#[arg(from_str = self)]`).** Rejected: a key that exists only
  because a macro cannot see traits. The probe answers, as in ADR-016 §3.
- **Always parse through `T: FromStr`.** Rejected: every `Title(String)` would
  need a `FromStr` that only unwraps.
- **The field first, the type as fallback.** Rejected: the wrong precedence;
  the outer type is the specific one.
- **Name the argument as a suffix** (`must be at least 1 (--limit)`). Rejected:
  it reads badly after `UnknownChoice`'s question mark. The prefix matches
  bpaf's own ``field: message`` shape.
- **Put the names on their own line, like bpaf's `[default: …]`.** Not
  reachable: `Meta::Suffix` is private to bpaf 0.9.28.
- **A labelled list (`[possible values: …]`).** Rejected: longer, and a
  bracketed list after a choice's help is not ambiguous.
- **Spell enums with data too.** Deferred: no consumer code has one.

## Consequences

- A spelled type with its own `FromStr` parses differently from 0.5. A word
  0.5 accepted may now be rejected — by the type's own rule. Listed as
  breaking.
- Spelled parse errors and `Choice` help rows change: snapshot tests are
  re-blessed (README, `examples/playr.rs`, the site, `tests/`).
- A newtype whose field has no `FromStr` and which has none of its own is
  E0599 naming both bounds (was E0277 on the field).
- Spelling keys on a struct with several fields no longer fail with
  "spelled by #[derive(Bpaf)]"; without `FromStr` they fail with E0277.
- `complete = files`/`dirs` on a value that is not a path now reads a `String`
  and parses it, so its error is named; nothing else changes.
- Depends on bpaf facts verified against 0.9.28: `complete_shell` keeps its
  directive whether or not the parse that follows fails; `.help` takes a `Doc`;
  monochrome rendering prints literals as plain text.
