# ADR-010: A file group is `#[group]` on the declaration

## Status

Accepted (2026-11-03)

Supersedes ADR-005 §5, and ADR-005's Consequences items about `group_mod!`, the
Rust 1.97 E0658 and MSRV 1.88. ADR-005 §§1–4 and 6–8 stand.

## Context

ADR-005 gave file modules a function-like shim,

```rust
group_mod! {
    /// events and instances
    pub mod event;
}
```

because rustc rejected an attribute macro on an out-of-line module (E0658,
"file modules in proc macro input are unstable", rust-lang/rust#54727) before
1.99.0, and a `#[proc_macro]` cannot share the name `group` with the
`#[proc_macro_attribute]` (E0428/E0252). ADR-005 planned to add
`#[group] pub mod x;` as an alias at 1.99 and deprecate `group_mod!`.
`#[group]`'s semicolon arm has routed to the file-form code ever since,
unreachable.

The toolchain pin is now 1.99.0 — attribute macros on non-inline modules, no
longer gated as of Rust 1.99.0 (rust-lang/rust#54727) — and a spike confirmed
that the attribute receives the bare declaration (`content: None`) with no
feature gate: the doc comment and the attribute's arguments arrive,
`Span::local_file` finds the declaring file, and nested declarations resolve.
An inner `#![group]` is still unstable (E0658).

0.1.0 is unpublished and unpushed: there are no consumers.

## Decision

### 1. `#[group] pub mod x;` is the file form, and the only one

```rust
/// events and instances
#[group]
pub mod event;

/// identity
#[group(ctx = Ctx, alias = "ident", hide)]
pub mod id;

/// the odds and ends
#[group]
#[path = "group_files/odds.rs"]
pub mod tools;
```

The same rules as an inline group: the name from the ident, a mandatory doc
comment, the `# Examples` footer, the keys `ctx`/`alias`/`hide`. `#[group]` reads
the declaration's doc comment, takes `ctx`/`alias`/`hide` as the attribute's
arguments, resolves the file by the Reference's rules, reads and parses it,
discovers the children, and emits the declaration with a synthesised relative
`#[path]` plus the `__hicli_x` handle — what `group_mod!` did, reached through
the attribute. One declaration per item.

`#[group]` re-emits the declaration (with a relative `#[path]`) for rustc to
load; the file is read by the macro only to discover children and the context.
It is never inlined, so diagnostics keep their real spans, rust-analyzer edits
the real file, and cargo's dep-info tracks it.

The `#[path]` is kept (amended by 0.2.1, see Amendment) even though a plain
`mod x;` resolves on 1.99, because it settles the case of a crate root not named
`lib.rs`/`main.rs` (e.g. `tests/group_files.rs` beside `tests/group_files/`).

### 2. `group_mod!` does not exist in 0.1.0

Removed, not deprecated, together with its mechanisms: the separate name,
reading and stripping `#[group(…)]` off a wrapped declaration, multi-declaration
invocations, discovery of `group_mod!` bodies, and its two diagnostics.
`hicli::group_mod` no longer exists.

### 3. MSRV 1.99

`rust-version = "1.99"` in both `Cargo.toml` and `macros/Cargo.toml`. Older
toolchains are not supported.

### 4. Unchanged from ADR-005 §5

- The declaring file is found via `Span::local_file()`/`Span::file()` and the
  anchor search.
- The emitted `#[path]` remains (see 1) (amended by 0.2.1, see Amendment).
- A file group must be at its file's top level. Inside an inline
  `#[group] mod a { … }`, a `#[group] mod b;` without `#[path]` is now rejected,
  since rustc would resolve it against a different directory than hicli can see.
- cargo's dep-info triggers rebuilds when the file changes.

## Why

- **One concept, one spelling.** A group is an annotated module, inline or file.
- **Subtraction.** A macro, two diagnostics, key forwarding, a discovery arm and
  the last surface rustfmt could not format all go.
- **Nothing to migrate.** No one has built against `group_mod!`.

## Alternatives considered

- **Alias and deprecate (ADR-005's plan)** — rejected: deprecation serves
  existing users, and there are none.
- **Keep `group_mod!` for pre-1.99 toolchains** — rejected by the owner: hicli
  targets current stable.
- **Inline the file's contents into the expansion (possible on 1.99)** —
  rejected: spans collapse to the `#[group]` line, rust-analyzer would analyse an
  invisible copy, and rebuild tracking would need the unstable `tracked_path`.

## Consequences

- Rust < 1.99 cannot build hicli.
- An inner `#![group]` remains unavailable (E0658, rust-lang/rust#54726).
- rust-analyzer's behaviour on attribute macros over out-of-line modules is still
  to be checked manually (`ROADMAP.md`).

## Amendment (0.2.1, #25)

rustc treats any file loaded through `#[path]` as a mod-rs file, so emitting `#[path]` on every file group made each group file mod-rs: a plain `mod helper;` in `src/source.rs` resolved to `src/helper.rs`, not `src/source/helper.rs` (E0583). `#[group]` now emits the synthesised `#[path]` only when rustc's own rules would not load the file the macro read. A declaration goes out as written when it is in a `mod.rs`/`lib.rs`/`main.rs` and the file is beside it, or when it is in a non-mod-rs `foo.rs` that hicli can prove rustc loaded without a `#[path]` and the file is in `foo/`. hicli proves this by finding a top-level, `#[path]`-free `mod foo;` in the file that owns `foo.rs`'s directory, recursively. In both cases the other arrangement must be absent. Everywhere else — a crate root not named `lib.rs`/`main.rs` with a same-named directory, a group nested in a `#[path]`-loaded file, an ambiguous `x.rs` + `x/mod.rs` — the `#[path]` is still emitted, and inside such a file rustc's mod-rs rule applies to plain submodules. The guarantee that rustc reads the bytes the macro read now holds by prediction where hicli can prove it, and by construction elsewhere; a wrong prediction fails to compile rather than compiling a different file.
