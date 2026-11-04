# ADR-005: A verb is the function; a group is a module

## Status

Proposed

Implemented 2026-09-17 (steps 1–3); the `group_mod!` name was confirmed by
spike (a `#[proc_macro]` and a `#[proc_macro_attribute]` cannot share a name,
E0428/E0252).

Extended by ADR-006 (2026-09-23): the snake_case-constructor rule moved from
`#[verb]` into `#[derive(Arg)]`.

"Currying stays additive" (Consequences) realised by
[ADR-011](011-curried-commands-and-default-commands.md) (2026-11-03):
`#[group(args = T)]` and `#[command(curried)]`.

Supersedes ADR-004's decisions 2 (`group!` as a list), 3 (`commands!` as a
`macro_rules!` macro) and 4 (`app!`'s forwarding), and ADR-004's decision 1
clause that `#[verb]` expands to "a function `create()` and a module `create`
holding `create::run`".

**ADR-004's decision 1 otherwise stands unchanged** — a subcommand is a
documented function; name from the ident, help from the doc comment, arguments
from owned `#[derive(Bpaf)]` parameters, context from the single `&T`, return
via `IntoOutcome`, no `#[arg(...)]` parameter attributes. **ADR-004's decisions 5
(`leaf` → `command`) and 6 (the runtime: `Verb` as a struct, `Success`/`Failure`,
`ExitCode` from `run`, the separate macro crate) stand unchanged.**
ADR-001 (help is mandatory) is strengthened again: a group's missing or blank
doc comment becomes a compile error, where it was a runtime panic.

Vocabulary renamed by [ADR-007](007-hicli-and-the-command-attribute.md) (`verbandi` → `hicli`, `#[verb]` → `#[command]`, `Verb` → `Invocation`, `VerbResult` → `Outcome`). The text below is kept exactly as written.

§5, and the Consequences items about `group_mod!`, the 1.97 E0658 and MSRV 1.88, superseded by [ADR-010](010-file-groups-are-an-attribute.md) (2026-11-03).

## Decision

### 1. `#[verb]` leaves the function alone

`#[verb]` emits the annotated function **verbatim** — same name, same signature,
same visibility, same attributes, same body — and *adds* one hidden sibling in
the value namespace:

```rust
/// create an event
#[verb]
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> VerbResult { BODY }
```

becomes

```rust
/// create an event
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> VerbResult { BODY }

#[doc = "`verbandi` registration handle for [`create`]."]
#[doc(hidden)]
#[allow(unreachable_pub)]
pub fn __verbandi_create() -> ::verbandi::Command<Ctx> {
    ::verbandi::command(
        "create",
        "create an event",
        ::verbandi::bpaf::construct!(event_dir(), event_name(),),
        |(a0, a1), ctx: &Ctx| create(a0, a1, ctx),
    )
}
```

The generated `mod create { pub fn run(…) }` is gone. Every other rule of
ADR-004's `#[verb]` — the name rule, the doc rule, the arity/`construct!`
trailing-comma rules, the context rule, the generic-over-`Ctx` case, every
rejection — is unchanged. `#[cfg(...)]`/`#[cfg_attr(...)]` on the function are
now copied onto the handle, so a cfg'd-out verb leaves no dangling handle.
`#[verb(name = "…")]` continues to set only the command-line name; the handle is
always derived from the ident.

### 2. The registration handle is `__verbandi_<ident>`, always `pub`

One convention for verbs and groups. Always `pub` and `#[doc(hidden)]`,
regardless of the annotated item's visibility, because a group handle in the
parent scope must reach `event::__verbandi_create()` at any nesting depth. `pub`
inside a private module is still effectively private, so a private verb's handle
is reachable exactly where the private verb was.

A group handle **reaches exactly one level down** — `__verbandi_top()` names
`top::__verbandi_event()`, never `top::event::__verbandi_create()` — which is
what makes "always `pub`" sufficient rather than merely convenient.

The name is public API in fact: `commands![other::verbs::event]` rewrites to
`other::verbs::__verbandi_event()`, and a handle may be written by hand
(§ "Consequences") to register a `Command` the macros cannot express.

### 3. A group is an annotated module

```rust
/// events and instances
///
/// An event is a directory; an instance is a branch of it.
#[group]
pub mod event {
    /// create an event
    #[verb]
    pub fn create(…) -> VerbResult { … }

    /// manage audiences
    #[group]
    pub mod audience { … }
}
```

- **Name** = the module ident, `_` → `-`.
- **Help** = the module's doc comment, split by the same rule as `#[verb]`'s:
  first line is the one-line help and the `--help` heading, the rest is
  `details`. A `#[group]` without a doc comment, or with a blank one, is a
  **compile error**.
- **Children** = every `#[verb]` function and every `#[group]`/`group_mod!`
  module in the body, **in source order, discovered automatically**. A child is
  a child because it is in the module.
- **Expansion** = the module verbatim (its inner `#[verb]`/`#[group]`
  attributes untouched, for rustc to expand next) plus a sibling
  `__verbandi_event()` in the parent scope, which pushes
  `event::__verbandi_create()`, `event::__verbandi_audience()`, … into a
  `Vec<Command<Ctx>>` and hands it to `verbandi::group`. Each child's
  `#[cfg(...)]` attributes ride on its `push` statement.

The module keeps its doc comment, so the group's help and the module's rustdoc
are one piece of text.

`#[group]` the attribute and `verbandi::group` the function share a name across
the macro and value namespaces and do not collide; one `use verbandi::group;`
imports both.

### 4. A group's context type

In order: `#[group(ctx = T)]`; otherwise the first `&T` found in a depth-first,
source-order walk of the children (recursing only while still looking); otherwise
the handle is **generic over `Ctx`**, exactly as a context-free `#[verb]`'s is.

A lifted type must be nameable in the parent scope: a single ident, a `crate::…`
path or a leading-`::` path. A module-relative path (`super::Ctx`) is a compile
error naming `#[group(ctx = …)]`.

### 5. File modules use `group_mod!` until Rust 1.99

`#[attr] pub mod x;` is feature-gated until Rust 1.99.0 (stable 2026-10-01); the
toolchain is pinned at 1.97.0. Until then:

```rust
group_mod! {
    /// events and instances
    pub mod event;
}
```

The macro emits `pub mod event;` — with its doc comment and a synthesised
`#[path]` — plus the same sibling handle, and discovers children by **reading
the file**, resolved by the Reference's rules: mod-rs files (`mod.rs`, and the
crate root) search their own directory; a non-mod-rs file `x.rs` searches `x/`;
`foo.rs` or `foo/mod.rs`; an explicit `#[path = "…"]` on the declaration wins and
is relative to the declaring file's directory. The file is parsed with
`syn::parse_file`, walked, and recursed into. One or more declarations per
invocation are accepted.

The declaring file is found via `proc_macro::Span::call_site().local_file()`
(stable since Rust 1.88, which becomes verbandi's MSRV), falling back to
`Span::file()`.

The macro **emits `#[path = "<relative>"]`** for every declaration it resolved
itself, so rustc reads exactly the bytes the macro read. This also removes the
one question a proc macro cannot answer — whether the declaring file is a crate
root not named `lib.rs`/`main.rs`, as `examples/demo.rs` and `tests/cli.rs` are.
No absolute path ever appears in the output.

Cargo already re-runs the macro when a child file changes, because the file is
`mod`-linked into the crate and therefore in dep-info. No `tracked_path` (which
is unstable) is needed.

**It is named `group_mod!`, not `group!`, because a `#[proc_macro]` and a
`#[proc_macro_attribute]` in one crate share the macro namespace and cannot both
be called `group`** (to be confirmed by spike; if they can coexist, the name
reverts to `group!`). The name says what it is — `#[group]`, for a `mod`
declaration — and is visibly a shim.

**When the pin moves past 1.99**, `#[group] pub mod event;` is added as an alias
and `group_mod!` is deprecated. This is purely additive: `#[group]` already
routes an `ItemMod` with a semicolon to the file-form code path (an arm rustc
makes unreachable today), and the body of `group_mod!` is *exactly* the item the
attribute will be written on, so migration is deleting the wrapper.

### 6. `commands!` becomes a proc macro

A **bare path** names a handle: `x` → `__verbandi_x()`, `a::b` →
`a::__verbandi_b()`. **Anything else** is an expression, passed through
`IntoCommand::into_command`. The result is a `::std::vec![…]`; an empty list is
`Vec::new()`; a trailing comma is accepted.

`app! { globals: …, commands: [ … ] }` stays `macro_rules!` and delegates, but
forwards its list as `$($c:tt)*` — an `:expr` capture would wrap each element in
an invisible group and destroy the bare-path distinction. Any consumer macro
forwarding into `commands!` must do the same.

`cli!` and `verb_test!` are unchanged, including their syntax.

### 7. Deletions

The list-form `group! { ctx: Ctx; event: [a, b] }`, `__group_fn!`, the
`macro_rules!` `commands!`, `src/macro_support.rs` (and the `__macro_support`
re-export), and `tests/ui/group_missing_doc.*` are removed. Dashing an ident and
splitting a doc comment now exist once, in the proc-macro crate, beside
`parse::read_doc` and `parse::snake_case`.

`verbandi::group`'s runtime panic on an empty group **stays**, for hand-built
trees. The macro path now refuses an empty group at compile time.

### 8. No `#[arg(...)]` parameter attributes

Confirmed again. Arguments remain consumer-written `#[derive(Bpaf)]` structs.

## Why

- **A verb whose function has been renamed is not a verb.** ADR-004's promise is
  that the function *is* the command. `mod create { fn run }` broke that at the
  moment it mattered most: `create(dir, name, &ctx)` — the call a test, a sibling
  verb, or the author's own recursion would write — did not exist. `create::run`
  is a trait in a mod's clothing: one required name, one required shape,
  reintroduced by the macro after ADR-004 removed it from the consumer. Leaving
  the function alone deletes that, and with it three smaller costs: `use
  super::*` in a synthetic module (which is why `#[verb]` had to be at module
  level), a `run_visibility` rule nobody should have to know, and ADR-004's
  recorded wart that a verb calling itself recursively hit the handle.
- **The tool chain sees the real function.** rust-analyzer's goto-definition,
  rename, find-references and inlay hints, and `cargo doc`, now land on the
  declaration the author wrote rather than on a re-spanned copy.
- **A group's list was a second place to write the tree.** `event: [create,
  roster, audience]` restated, in a non-Rust grammar, what the module already
  said. Every new command cost a function *and* a registration line, and the two
  could disagree — a verb written and never listed is a silent no-op, which is
  the failure the "no central dispatch match" rule exists to prevent. Membership
  by containment cannot disagree with itself.
- **rustfmt comes back.** ADR-004 accepted "`group!` bodies are not
  rustfmt-formatted" as a papercut. With `#[group]` on a module, the entire group
  — every verb in it — is ordinary Rust that rustfmt formats. The unformatted
  surface shrinks to the two-line `group_mod!` shim, which disappears at 1.99.
- **`ctx: Ctx;` was ceremony for a type the children already name.** A group's
  context is whatever its verbs take; reading it from the first `&T` deletes the
  header, and falling back to a *generic* handle (rather than to a type named
  `Ctx` being in scope) makes a context-free group mount anywhere, by the same
  rule `#[verb]` already used.
- **Help stops being checkable-at-runtime.** `split_doc` panicked at the handle's
  call site on a blank doc comment because `macro_rules!` can see an attribute
  but not its text. A proc macro can, so ADR-001's rule is enforced by rustc for
  groups as it already was for verbs, and an entire `#[doc(hidden)]` support
  module disappears.
- **Two spellings of the dash rule and of the doc split became one.** `dash`,
  `as_str` and `split_doc` in the runtime crate existed only because
  `macro_rules!` cannot rewrite an ident or read a literal. They duplicated
  `parse::snake_case` and `parse::read_doc`, with the ordinary risk of drifting.

## Alternatives considered

- **Keep `mod create { run }` and add a `pub use create::run as create_impl;`.**
  Rejected: it adds a third name for one function and still leaves the verb's own
  name occupied by the handle.
- **Name the handle `__verb_x`.** Rejected: the name crosses crate boundaries in
  `commands![other::verbs::event]`, so it is a public contract and should carry
  the crate's name; and `__verb_event` for a group reads wrong.
- **Make the handle's visibility mirror the item's.** Rejected: a group handle
  in the parent scope must reach into a possibly-private child module, and
  computing the right restriction per nesting depth is a rule nobody can hold in
  their head. `pub` inside a private module is already private.
- **A single `group! { pub mod event { … } }` covering both the inline and file
  forms.** Rejected: a brace-delimited macro body is not formatted by rustfmt, so
  the inline form would lose formatting for every verb inside it — the main
  benefit of the change.
- **`#[group] pub mod event {}` (empty body) meaning "read the file".**
  Rejected: it is unmarked magic, and an author who genuinely wrote an empty
  group gets "file not found" instead of "this group has no commands".
- **Registering children explicitly inside the module** (`#[group] mod event`
  plus `commands![create, roster]` in the body). Rejected: it is the ADR-004 list
  again, moved.
- **`inventory`/`linkme` auto-registration.** Still rejected. Discovery here is
  lexical and local — the macro reads one module body or one file and can be
  followed by eye — not link-time collection across a binary.
- **Erroring when a group's children name two different context types.**
  Rejected: `Ctx` and `crate::Ctx` are the same type with different tokens, so a
  token-equality check would produce false compile errors. The first wins, and
  the second produces an ordinary `Command<A>`/`Command<B>` mismatch spanned to
  the offending child.
- **Making `commands!` accept a variable holding a `Command` as a bare name.**
  Impossible: a variable and a handle are the same syntax. A bare path means a
  handle; a value is written `(c)` or passed to `App::commands(vec![…])`.

## Consequences

- **Breaking, wholesale, against the in-repo 0.1.0.** Still unpublished, still
  zero external consumers. In-repo call sites: `src/lib.rs`, `src/macros.rs`,
  `src/command.rs`, `examples/demo.rs`, `tests/cli.rs`, `tests/macros.rs`,
  `tests/verb_macro.rs`, `tests/ui/*`, `README.md`, `docs/PLAN.md`,
  `docs/sketches/moniker.rs`, `CLAUDE.md`.
- **MSRV becomes Rust 1.88** (`Span::local_file`), recorded as
  `rust-version = "1.88"`. Adopters on older toolchains can use inline
  `#[group]`, which needs no file access, but the crate as a whole will not
  build.
- **`App::command(create)` no longer compiles**: `create` is the function.
  Register with `app!`/`commands!`, or `App::command(__verbandi_create)`.
- **`commands![c]` where `c` is a local no longer compiles**; write `(c)` or
  `App::commands(vec![c])`.
- **An external, or any hand-built `Command`, cannot be a discovered child.**
  Write the group's handle by hand — `#[doc(hidden)] pub fn __verbandi_event()
  -> Command<Ctx> { verbandi::group(…, commands![…]) }` — which `commands![event]`
  then finds. The handle convention is documented public API for exactly this.
- **A group's child order is its source order.** Moving a command in `--help`
  means moving a function.
- **`group_mod!` must be at a file's top level.** Rust resolves `mod x;` inside
  an inline `mod a { … }` against `a/`, and a proc macro cannot see the inline
  module path it sits in. Nested use fails loudly with "cannot find the file for
  module", whose note says so; use an inline `#[group] mod` there.
- **`group_mod!` needs the compiler to disclose the source path.** Under
  `--remap-path-prefix`, or any front end that does not implement
  `Span::local_file`/`Span::file`, the macro reports that it cannot locate the
  file and points at the inline form.
- **The declaring file's path is anchored on rustc's working directory, which
  cargo sets to the workspace root, not the package root** (confirmed by
  spike). `group_mod!` therefore joins `Span::local_file()`/`Span::file()` onto
  `std::env::current_dir()`, never `CARGO_MANIFEST_DIR`, which would be silently
  wrong for any package but the workspace root's own.
- **On the pinned Rust 1.97, `#[group] pub mod event;` reports rustc's own
  E0658 (feature-gated attribute) first, and only then the file-form
  diagnostics `#[group]` itself would raise** — the arm that routes an
  `ItemMod` with a semicolon to `group_mod!`'s code path exists in the macro
  today but is unreachable until the toolchain pin reaches Rust 1.99, because
  rustc rejects the attribute on the item before the macro ever runs.
- **`#[verb]` may now be written inside a function body.** Both emitted items are
  ordinary items. `#[group]` discovery still only sees module-level items.
- **A verb calling itself recursively now calls itself.** ADR-004's recorded
  papercut is repealed.
- **Currying stays additive.** ADR-004's three shapes are untouched — `Mount`,
  `Verb::run`, and "the body is `create(dir, name, ctx)` either way" (now
  literally the function). The syntax slot moves from `event(EventRef): [...]`
  to `#[group(args = EventRef)]`, which is a better place for it: an attribute
  argument beside `ctx = …` rather than a position in a bespoke grammar.
