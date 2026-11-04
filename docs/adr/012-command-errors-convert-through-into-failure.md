# ADR-012: Command errors convert through `Into<Failure>`

## Status

Accepted (2026-11-04)

Closes johlrogge/hicli#6 (`?` on application errors). Amends ADR-004: its
four concrete `IntoOutcome` impls become two concrete and two generic ones.
The success side stays closed.

## Context

mdma-cli's commands are written against `color_eyre::Result<()>`, like most
binaries that use eyre or anyhow. Under 0.1.0 a command had to return
`Outcome` (`Result<(), Failure>`), so every fallible call became
`.map_err(Failure::runtime)?`. That `Failure::runtime` also renders only
`Display`, which drops every `wrap_err` / `context` layer under the outermost.
The alternative was a wrapper function per command, which ADR-005 removed:
`fn sync(..) -> color_eyre::Result<()>` *is* the command, or nothing is.

## Decision

### 1. Any error is a runtime failure, with its chain as the message

```rust
impl<E> From<E> for Failure
where
    E: Into<Box<dyn std::error::Error + 'static>>,
{
    fn from(e: E) -> Self { /* Failure::Runtime(chain(&*boxed)) */ }
}
```

The conversion yields `Failure::Runtime`, exit code `1`. The message is the
error's `Display`, then each `source()` in turn, joined by `": "`:
`eyre!("root").wrap_err("mid").wrap_err("outer")` and
`anyhow!("root").context("mid").context("outer")` both print
`outer: mid: root`.

### 2. `IntoOutcome`'s `Result` impls are generic over the error

```rust
impl<E: Into<Failure>> IntoOutcome for Result<(), E>      { … }  // Ok(()) => Success::OK
impl<E: Into<Failure>> IntoOutcome for Result<Success, E> { … }  // map_err(Into::into)
```

These replace the concrete impls for `Outcome` and `Outcome<Success>`. The
`()` and `Success` impls are unchanged. `Failure` reaches these impls through
core's reflexive `From<T> for T`, not through the blanket, so a
`Failure::Usage` keeps exit code `2`.

### 3. `Failure` never implements `std::error::Error`

The blanket in §1 coheres with core's `impl<T> From<T> for T` only because
`Failure: Into<Box<dyn Error>>` does not hold, and rustc can see that it
never will: `Failure` is local, and it implements neither `Error` nor
anything else std boxes. Implementing `Error` for `Failure` would make the two
impls overlap. This is a permanent rule, documented on the type. `Failure`
implements `Display`, which is all a printer needs.

### 4. No downcasting back to a code

A boxed error never becomes a `Failure::Usage`, and no error type gets a
custom exit code through `downcast_ref`. A usage error is detected by the
argument's `Arg` parser (where bpaf reports it with exit `2`) or by a command
that returns `Outcome` and says `Failure::usage(..)`. A command that wants
a different code returns `Ok(Success::code(n))`.

### 5. No eyre or anyhow features

hicli depends on neither crate. `anyhow::Error` and `eyre::Report` each
implement `From<_> for Box<dyn Error + 'static>` (and for the `Send + Sync`
box), so the bound in §1 covers both, with no feature and no dependency.
They are dev-dependencies only, for `tests/errors.rs`.

## Spike findings (2026-11-04, rustc 1.99.0, anyhow 1.0.104, eyre 0.6.14)

- **Coherence holds.** The blanket `From` beside core's reflexive impl
  compiled first time. No E0119.
- **Accepted with `Into<Box<dyn Error + 'static>>`:** `eyre::Result<()>`,
  `anyhow::Result<()>`, `io::Result<()>`, `Result<(), Box<dyn Error>>`,
  `Result<(), String>`, `Result<(), &'static str>`, and `Outcome` itself.
- **Not accepted:** `Result<(), Box<dyn Error + Send + Sync>>`. A
  `Box<dyn Error + Send + Sync>` does not implement `Error` (the impl for
  `Box<T>` needs `T: Sized`), and std has no `From` that drops the auto
  traits from a boxed trait object. The reverse bound,
  `Into<Box<dyn Error + Send + Sync + 'static>>`, was also tried. It accepts the
  `Send + Sync` box and rejects plain `Box<dyn Error>` symmetrically, and it
  also rejects any non-`Send` error. The `'static` bound was kept, as both
  report types pass it and it does not require errors to be thread-safe.
- **Rendering:** both chains above render exactly `outer: mid: root`.
- **Breaking:** `hicli::command("x", "x", pure(()), |(), _: &()| Ok(()))`
  now fails with E0282 "type annotations needed" (cannot infer `E`). No
  call site in the repository had that shape.
- **Diagnostics:** `#[diagnostic::on_unimplemented]` on `IntoOutcome`
  fires for a wrong *shape* (`-> i32`, `-> Result<String, Failure>`). For a
  right shape with a wrong error type, rustc reports the leaf obligation
  instead: "the trait bound `NotAnError: std::error::Error` is not satisfied",
  followed by a note chain ending "required for `Result<(), NotAnError>` to
  implement `IntoOutcome`". hicli cannot attach a message to that, but the
  error still names the missing trait.

## Why

- **The function stays as written (ADR-005).** `fn sync(..) ->
  color_eyre::Result<()>` is the command. Called directly, it still returns
  an `eyre::Report`. Only the run path converts, at the point it already
  calls `into_outcome()`.
- **One bound covers everything.** Std errors, `thiserror` enums, both report
  types, `Box<dyn Error>`, `String` and `&str` all go through std's own
  `Into<Box<dyn Error>>`. Nothing is listed per crate, and a future error
  library that interoperates with `Box<dyn Error>` works unchanged.
- **The success side stays closed.** The open-endedness ADR-004 warned against
  was a set of legal *return shapes*. That set is still four, and readable on
  one page. Only the error parameter is generic, and its bound is a single
  well-known std conversion.

## Alternatives considered

- **A blanket over `E: std::error::Error` only.** It doesn't reach eyre or
  anyhow: neither `Report` nor `anyhow::Error` implements `Error`, by design.
  Consumers couldn't bridge them either, because the orphan rule forbids
  `impl From<eyre::Report> for hicli::Failure` in a third crate.
- **Cargo features (`eyre`, `anyhow`)** with a concrete `From` each. This
  works, but adds optional dependencies and a feature matrix to test, and it
  only covers the crates named in it. The `Box` route needs none of that.
- **A `Display` bound** (`impl<E: Display> From<E> for Failure`). It overlaps
  with core's `From<Failure> for Failure`, because `Failure: Display`. Any
  bound a `Failure` satisfies would send a `Usage` through the blanket and
  turn it into a `Runtime` (E0119 aside). A `Display` bound would also lose
  the source chain.
- **`{:?}` rendering.** eyre and anyhow put the chain in `Debug`, but with
  backtraces, `Caused by:` blocks and, under color_eyre, ANSI colour. That
  is developer output, not a one-line message for the user.
- **A structured chain in `Failure`** (`Runtime { message, causes: Vec<String> }`).
  Deferred: nothing prints the parts separately yet, and
  `Failure::Runtime(String)` stays unchanged.

## Consequences

- **Breaking in 0.2.0 for unannotated closures.** A closure passed to
  `hicli::command` / `curried_command` that returns `Ok(())` no longer
  infers `E = Failure`. Write `|args: A, ctx: &Ctx| -> Outcome { …; Ok(()) }`.
  `#[command]` fns are unaffected, since their return type is written. This is
  documented on both functions.
- **`Box<dyn Error + Send + Sync>` is not accepted.** Return `Box<dyn Error>`,
  `anyhow::Error` or `eyre::Report` instead, or map the error with
  `Failure::runtime`. This is documented on `IntoOutcome`.
- **An error whose `Display` already includes its source shows it twice**
  (`read config: No such file: No such file`). anyhow's `{:#}` and eyre's
  chain printing do the same; the fix belongs in that error's `Display`.
- **`Failure::runtime` stays**, for a message spelled on the spot or a
  `Display` value that is not an error. Its docs point to `?` first.
- **New tests:** unit tests in `src/outcome.rs` (a hand-written two-level
  `source()` chain, `io::Error` and `String` becoming `Runtime` with code `1`,
  `Failure::usage` keeping `2`, `Ok(Success::code(7))` under a foreign error),
  `tests/errors.rs` (in-process, through `App::run_with_args` and
  `Invocation::run`), and `trybuild` cases `return_not_an_outcome`,
  `return_result_of_a_value` and `return_error_not_an_error`.
