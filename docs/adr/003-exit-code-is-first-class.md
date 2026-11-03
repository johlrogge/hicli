# ADR-003: A verb's exit code is a value, not a `process::exit`

## Status

Accepted (2026-09-11)

**Superseded by [ADR-004](004-function-first-verbs.md) (2026-09-16).** `Exit`
is replaced by `Success`/`Failure` (`src/outcome.rs`): `Success(u8)` for
"finished, exit with this code" and `Failure::{Usage, Runtime}` for "failed,
print this". `run` (a verb's, and `App::run`) returns `std::process::ExitCode`
on the outermost path. Every substantive decision below survives the split
intact — a code is a value, `std::process::exit` never appears in the library,
a signal death is `128 + N` — this ADR's `Code(i32)` variant and its
`Code(0)` → `1` normalisation are what's gone: a success code now lives on the
`Success` side, where `Success::code(0)` is simply `Success::OK` and needs no
normalising, and `message()` no longer needs `Option` because `Failure` (unlike
old `Exit`) has no silent variant. See ADR-004 for why.

Vocabulary renamed by [ADR-007](007-hicli-and-the-command-attribute.md) (`verbandi` → `hicli`, `#[verb]` → `#[command]`, `Verb` → `Invocation`, `VerbResult` → `Outcome`). The text below is kept exactly as written.

## Decision

`Exit` gains a third variant and `message()` changes shape (pre-ADR-004 — see
Status):

```rust
#[non_exhaustive]
pub enum Exit { Usage(String), Runtime(String), Code(i32) }

impl Exit {
    pub fn code(&self) -> i32;              // 2 / 1 / n, with Code(0) -> 1
    pub fn message(&self) -> Option<&str>;  // None for Code
}
```

`Exit::Code(n)` is **silent**: it carries no message, and `App::run` prints
nothing for it — not even a blank line. `ExternalRun::run` maps a child's
non-zero exit to `Err(Exit::Code(n))` instead of calling `std::process::exit`,
and on Unix maps a signal death to `Err(Exit::Code(128 + N))`. Spawn failure
stays `Exit::Runtime` (exit 1). `std::process::exit` no longer appears anywhere
in the library.

## Why

`ExternalRun::run` called `std::process::exit(code)` to forward a child's exit
code, because `Exit` could only encode 1 and 2. That one line broke three of
the crate's own contracts at once:

- **It skips destructors.** Anything the consumer's `Ctx` holds — a lock file, a
  flushed writer, an open transaction — is abandoned mid-flight, from inside a
  library the consumer merely called.
- **It bypasses the consumer's `main`.** `App::run` is documented to *return* an
  exit code so the consumer decides what to do with it. For exactly one verb, it
  didn't return at all.
- **It made that verb un-testable in-process.** "Testability is first-class and
  in-process, no subprocess spawning" is a stated design pillar; a call to
  `process::exit` cannot be observed by a test, only by dying. The external verb
  was the one verb whose `run` no test could call.

A code is data. Returning it as data is both smaller and more honest than
terminating the process as a side effect.

**Why `Code(i32)` and not `Code { code, message: Option<String> }`.** The need is
precisely "the child already said what went wrong; just carry the number out".
An optional message adds a second state nothing produces, and invites a caller to
print a message *on top of* the child's own stderr — duplicating the diagnostic.
If a consumer wants a message, `Usage`/`Runtime` already are that; wanting both a
custom message and a custom code is not a demonstrated need. Subtractive scope.

**Why `message()` returns `Option<&str>`.** The alternative — keep `-> &str`,
return `""` for `Code`, print conditionally on non-empty — encodes "there is no
message" as a string value, and puts the "don't print a blank line" rule in the
caller where it can be forgotten. `Option` puts it in the type: `App::run`
*cannot* print for a `Code` without writing an `unwrap`. It also makes the
silence assertable (`Code(7).message().is_none()`) without capturing stderr,
which a unit test cannot do.

**Why `Code(0)` normalises to `1`.** A verb that exits 0 returns `Ok(())`; an
`Err(Exit::Code(0))` is a contradiction. Making it unrepresentable would need a
private field or a fallible constructor, which costs more API than the case is
worth. Panicking in a library on a consumer's bad value is worse. Silently
returning 0 is the genuinely dangerous option — a failing command reported as
success, a red CI job turning green — so `code()` normalises it to 1. The
failure mode of the normalisation is a slightly wrong non-zero code; the failure
mode of the alternative is an undetected failure.

**Why `128 + N` for signal deaths.** The previous behaviour folded every signal
death into exit 1. `git`, `bash`, and every POSIX shell report `128 + N`, so
`app foo` interrupted with Ctrl-C reported `1` where running `foo` directly
reports `130` — the external model's entire promise is that `app foo` behaves
like `foo`, and a wrapper that rewrites the failure mode breaks it. On Unix this
is six lines behind `cfg(unix)` using `ExitStatusExt::signal()`; elsewhere
`ExitStatus::code()` is used unchanged. `verbandi` reports the code; it
deliberately does **not** re-raise the signal on itself, which is what a faithful
shell would do — that would be another hidden process side effect, and it is the
consumer's `main`'s decision.

**Why `#[non_exhaustive]` now.** The crate is unpublished, so this is the free
moment; after 0.1.0 both adding the attribute and adding a variant are breaking.
The enum has now grown once (2 → 3 variants) in the first hardening pass, and a
`Signal(u8)` variant is a live candidate the moment a consumer wants to
distinguish it from a plain code. The cost is that a downstream exhaustive
`match` needs a `_` arm; the mitigations — `code()` and `message()` — cover
every use the crate actually intends. It costs no API surface: it withholds a
guarantee rather than adding mechanism.

## Alternatives considered

- **Keep `process::exit`, document it.** Rejected: no amount of documentation
  makes a library-initiated process exit testable, and it contradicts
  `App::run`'s return type.
- **`Exit::Code { code, message: Option<String> }`.** Mechanism without a
  consumer; see Why. Reachable later without breaking `Code(i32)` callers only
  if we accept a fourth variant — acceptable under `#[non_exhaustive]`.
- **A fallible constructor `Exit::from_code(n) -> Option<Exit>` with a private
  field.** Makes `Code(0)` unrepresentable, but blocks downstream pattern
  matching (which tests do use) and trades a well-defined normalisation for an
  `unwrap` at every call site.
- **Signal death stays `Exit::Runtime` (exit 1).** Simpler, one fewer `cfg`, but
  loses the distinction between "the child failed" and "the child was killed",
  and diverges from every shell. Rejected.
- **Test the mapping by spawning `/bin/sh -c "exit 7"`.** Rejected: it would be
  the only real subprocess in a suite whose stated rule is that there are none,
  and what it proves is that `std::process::Command` works — std's job. The
  mapping is extracted as `exit_from_status(name, status)` and tested against
  `ExitStatus`es built with `ExitStatusExt::from_raw` instead, which exercises
  exactly the logic this ADR introduces with no process boundary.

## Consequences

- **Breaking against the in-repo 0.1.0**, twice over: `message()` changes return
  type, and `#[non_exhaustive]` closes exhaustive downstream matching. Zero
  external consumers; the in-repo call sites are `App::run` and `Display for
  Exit`.
- `Display for Exit` and `Exit::message()` now differ deliberately: `Code(7)`
  displays as `exited with code 7` (so a logged `Exit` is never blank) but has
  no `message()` (so `verbandi` prints nothing). Both are documented on the
  respective items.
- An external killed by a signal now reports `128 + N` instead of `1`. If a
  consumer scripted against the old `1`, that is a behaviour change — and the
  new value is the one the shell would have produced.
- The status mapping is extracted as a pure `exit_from_status(name, status)`,
  unit-tested on Unix; the `None` arm (no code, no signal) is unreachable in
  practice and kept as a defensive runtime error rather than a panic.
- **Remaining gap:** nothing asserts that `ExternalRun::run` hands the *child's*
  status to `exit_from_status` — three lines of wiring between a `Command` we
  will not spawn and a function we do test. Closing it needs either a real
  subprocess (rejected above) or a spawn seam on `ExternalRun`; neither is worth
  it until a consumer reports a mis-propagated code. (The spawn-failure half of
  that wiring is now exercised end-to-end through `run_with_args`; the
  success-status half remains untested.)
- **Resolved (2026-09-11):** `App::run_with_args(args, mk_ctx)` was added as the
  run-level test seam and `App::run` is now
  `run_with_args(std::env::args().skip(1), mk_ctx)`. Every branch of the run
  path, including `Exit::Code` propagation and `Code(0)` → 1, is covered
  in-process in `tests/cli.rs`. The seam returns the exit code but does not
  capture stdout/stderr, so the appended `did you mean` text stays covered by
  the `augment_stderr` unit tests; a writer-injecting variant was considered and
  rejected as API surface bought for one string assertion.
