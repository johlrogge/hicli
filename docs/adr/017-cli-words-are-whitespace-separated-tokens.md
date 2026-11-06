# ADR-017: `cli!` words are whitespace-separated source tokens

## Status

Accepted (2026-11-06)

Closes johlrogge/hicli#22 (`cli!` splits kebab-case words like `set-output`
and `--dry-run` into separate tokens). ADR-014 is reserved for
johlrogge/hicli#7; ADR-016 covers 0.5.0's spelled argument types.

## Context

- **`cli!` was a `macro_rules!` token-muncher** (`__cli_munch!`). It walked
  the input one token tree at a time with a handful of rules: `- - ident` is a
  long flag, `- ident` a short flag, `- literal` a negative number, a literal
  its `Display` form, anything else `stringify!`'d. The Rust lexer splits
  `set-output` into `set`, `-`, `output`, so it came out as `["set",
  "-output"]`, and `--dry-run` as `["--dry", "-run"]`. The README taught
  quoting every word with a hyphen.
- **A declarative macro cannot do better.** `macro_rules!` sees neither
  whitespace nor spans: `- $flag:ident` matches `-x` and `- x` alike, so it
  cannot tell `set-output` from `set - output`.
- **A proc macro can, on stable.** proc-macro2's `span-locations` feature
  exposes `Span::start()` and `Span::end()` as line/column. Inside a real
  macro expansion proc-macro2 (1.0.107, `build.rs`) answers them from
  `proc_macro::Span::{line, column}` on rustc 1.88 and later, which it
  detects with a stable compile probe; hicli's MSRV is 1.99.
- **`Span::join` is the other way to ask** ("is there one span covering both
  tokens?"), and it needs the nightly-only `proc_macro_span` feature. hicli is
  stable-only.

## Decision

### 1. A word is a run of tokens that touch

A **word** is a maximal run of top-level tokens where each token starts
exactly where the previous one ends: `prev.span().end() == next.span().start()`,
comparing line and column. Whitespace — a space, a line break, a comment —
separates words, which is the shell's rule. A word's text is its tokens' texts
concatenated:

- an ident or punct: its `to_string()`;
- a string or raw string literal: its value (`syn::LitStr::value`);
- a char literal: its value;
- any other literal: its source text as written (`0x10`, `1.50` and `5u8`
  stay as written).

An invisible group (`Delimiter::None`, what a `macro_rules!` forwarder wraps
an `:expr` in) is flattened in place; its tokens keep their own spans, so they
join their neighbours exactly as written. Any other group is a compile error,
``cli!: a bracketed group is not a word; quote it: "[a]"``. A byte string or
C string is a compile error, `cli!: a byte string is not a word; write a
"string literal"`.

| input | words |
|---|---|
| `playback set-output hdmi` | `["playback","set-output","hdmi"]` |
| `--dry-run` | `["--dry-run"]` |
| `-v` | `["-v"]` |
| `-5` | `["-5"]` |
| `--since=-7` | `["--since=-7"]` |
| `--name="My Event"` | `["--name=My Event"]` |
| `email,phone` | `["email,phone"]` |
| `a/b/c` | `["a/b/c"]` |
| `./ev` | `["./ev"]` |
| `-- foo` | `["--","foo"]` (was `["--foo"]`) |
| `--` | `["--"]` (was a compile error) |
| `a - b` | `["a","-","b"]` (was `["a","-b"]`) |
| `- 5` | `["-","5"]` (was `["-5"]`) |
| `""` | `[""]` |
| `x "My Event"` | `["x","My Event"]` |
| `"a""b"` | `["ab"]` (was `["a","b"]`) |
| empty | `[]` |

`'My Event'` is still a lexer error.

### 2. `cli!` moves to `hicli-macros`

`cli!` is `#[proc_macro] pub fn cli` in `hicli-macros`, re-exported as
`hicli::cli`. The reading lives in `macros/src/cli.rs`, in `proc_macro2`
only, so it is unit-tested outside an expansion; with `span-locations`,
`TokenStream::from_str` gives real line/column there too. It expands to

```rust
{ let __v: ::std::vec::Vec<::std::string::String> = ::std::vec![ ::std::string::String::from("…"), … ]; __v }
```

`__cli_munch!` is removed. `cli_test!` stays `macro_rules!` and forwards its
lines as token trees, which keep their spans.

### 3. The location guard

A real token is never zero-width. If either token of a pair has
`start == end`, the front end gave no location — proc-macro2 answers `0:0`
when it cannot ask, and rust-analyzer's proc-macro server may give dummy
spans — and the two are **separate** words. Without locations, `cli!` reads a
token per word: `set-output` is `["set", "-", "output"]` there, which is
never worse than the old muncher and only affects what an IDE shows for a
test's expansion, not what `cargo test` runs.

## Alternatives considered

- **`Span::join`.** Nightly-only (`proc_macro_span`). Rejected outright:
  hicli is a dependency stable-toolchain CLIs pin.
- **More `macro_rules!` rules** (`$a:ident - $b:ident` as one word). Cannot
  tell `a-b` from `a - b`, so it trades one surprise for another.
- **`Span::source_text`.** Stable, but returns `None` for tokens from a macro
  expansion, and reading the text back means re-lexing quotes and escapes.
  Comparing locations needs neither.
- **Keep quoting.** The rule the README taught; it was the reported pain
  (#22), and a test that cannot show the command line verbatim is not
  reading like the terminal.

## Consequences

- `cli! { playback set-output --dry-run }` is the command line as typed.
- Edge cases change (the "was" rows above) and non-string literals are kept
  as written instead of re-rendered through `Display`; listed under
  "Changed" in the changelog. Brackets and byte strings are now rejected.
- `span-locations` is enabled for every crate in the build that uses
  proc-macro2, through feature unification. The cost is negligible.
- `hicli::testing::to_arg` is no longer used by `cli!`; it stays public and
  `#[doc(hidden)]`.
- `#[macro_use] extern crate hicli;` still imports `cli!` (pinned by
  `tests/macro_use_probe.rs`).
