//! What a command reports back: [`Success`], [`Failure`] and [`IntoOutcome`].
//!
//! A command never calls `std::process::exit`. It *returns* its outcome, and
//! `hicli` carries that value out through [`App::run`](crate::App::run) so
//! destructors run and the consumer's `main` stays in control.
//!
//! The outcome is split in two, because "finished, exit with this code" and
//! "failed, print this" are different statements:
//!
//! - [`Success`] carries a code and nothing else. It is how an external
//!   subcommand forwards its child's exit code without hicli adding a word
//!   of its own.
//! - [`Failure`] carries a message that hicli prints to stderr, and a code
//!   derived from *which kind* of failure it is — never spelled by the command.
//!
//! The split replaces the single `Exit` enum of ADR-003. `Exit` had to
//! normalise `Code(0)` to `1`, because a code lived on the `Err` side where a
//! *successful* code had nowhere to go; `message()` returned an `Option`,
//! because one of its variants was silent. With the split both warts are gone
//! structurally: `Success::code(0)` is an ordinary success, a `Success` is
//! silent because it has no message to print, and [`Failure::message`] is
//! always a `&str`.
//!
//! A command need not spell `Failure` at all. Any error converts into one
//! (ADR-012), so `fn sync(..) -> color_eyre::Result<()>` or
//! `-> io::Result<()>` is a command as written, and its error is printed as
//! its message chain, `outer: mid: root`, with exit code `1`.
//!
//! Codes are `u8`, matching `std::process::ExitCode` and POSIX, which reports
//! only the low 8 bits of a child's status. `Exit::Code(256)` — a failure the
//! OS would have reported as success — is not representable here.

use std::fmt::Display;

/// A command finished. The `u8` is the process exit code, usually `0`.
///
/// `Success` deliberately carries **no message**. Anything worth telling the
/// user was already printed by the command (or, for an external subcommand, by
/// the child process); hicli prints nothing at all for a `Success`, not even a
/// blank line.
///
/// A non-zero success is not a contradiction: it is "this command has finished
/// and the process should exit with `n`", which is exactly what an external
/// subcommand forwarding its child's code needs, and what a `grep`-style
/// "nothing matched" result is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Success(u8);

impl Success {
    /// The ordinary success: exit code `0`.
    pub const OK: Success = Success(0);

    /// Finish with a specific exit code.
    ///
    /// `Success::code(0)` is legal and stays `0` — it is simply [`Success::OK`]
    /// spelled the long way. Nothing is normalised, because nothing here can
    /// lie: this value *is* the code the process will exit with.
    pub fn code(n: u8) -> Success {
        Success(n)
    }

    /// The process exit code this success carries.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// A command failed, with a message hicli prints to stderr.
///
/// The variant chooses the exit code — a command states *what kind* of failure
/// it hit, not a number:
///
/// - [`Failure::Usage`] — the invocation was malformed. Exit code `2`, matching
///   the code hicli remaps `bpaf`'s own parse errors to (see
///   [`exit_code_for`](crate::exit_code_for)), so a usage error detected by the
///   parser and one detected by a command are indistinguishable from outside.
/// - [`Failure::Runtime`] — the command ran and failed. Exit code `1`.
///
/// There is no silent variant and no caller-chosen code: "exit with `n`,
/// quietly" is a [`Success`], which is where a code can live honestly.
///
/// This enum is `#[non_exhaustive]`: match on it with a `_` arm, or prefer
/// [`Failure::code`] and [`Failure::message`].
///
/// Any error converts into a `Failure` — a [`Failure::Runtime`] carrying the
/// error's message chain — so `?` works on an `io::Error`, an `anyhow::Error`
/// or an `eyre::Report` inside a command returning [`Outcome`] (ADR-012).
///
/// `Failure` **never implements [`std::error::Error`]**, and that is
/// load-bearing, not an omission: the blanket
/// `impl<E: Into<Box<dyn Error>>> From<E> for Failure` only coheres with core's
/// reflexive `impl From<T> for T` because `Failure` itself does not convert
/// into `Box<dyn Error>`. Implementing `Error` for it would make the two
/// overlap and break every command that uses `?`. It implements `Display`,
/// which is all a printer needs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Failure {
    /// A usage error — the invocation was malformed. Exit code `2`.
    Usage(String),
    /// A runtime error — the command ran but failed. Exit code `1`.
    Runtime(String),
}

impl Failure {
    /// Build a [`Failure::Usage`] from any string-like message.
    pub fn usage(m: impl Into<String>) -> Self {
        Failure::Usage(m.into())
    }

    /// Build a [`Failure::Runtime`] from any `Display` value.
    ///
    /// Reach for `?` first: an error converts into a `Failure` on its own, with
    /// its whole source chain in the message. `runtime` is for the rest — a
    /// message spelled on the spot (`Failure::runtime("no such source")`), or
    /// a `Display` value that is not an error, e.g.
    /// `something().map_err(Failure::runtime)?`. It renders `Display` only,
    /// without the source chain.
    pub fn runtime<E: Display>(e: E) -> Self {
        Failure::Runtime(e.to_string())
    }

    /// The process exit code for this failure: `2` for usage, `1` for runtime.
    pub fn code(&self) -> u8 {
        match self {
            Failure::Usage(_) => 2,
            Failure::Runtime(_) => 1,
        }
    }

    /// The message hicli prints to stderr for this failure.
    ///
    /// Always present: every `Failure` says something, so there is no `Option`
    /// to unwrap and no silent-failure case to remember.
    pub fn message(&self) -> &str {
        match self {
            Failure::Usage(m) | Failure::Runtime(m) => m,
        }
    }
}

impl Display for Failure {
    /// The failure's message, verbatim — the same text hicli prints.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// The idiomatic return type of a command: `Result<T, Failure>`, defaulting to
/// `Result<(), Failure>`.
///
/// `Outcome` is the plain "worked or failed" shape; `Outcome<Success>` is
/// for the rarer command that finishes with a code of its own. Both are
/// accepted by [`IntoOutcome`], so a command picks whichever tells the truth
/// about it and the `?` operator works either way — including on any
/// [`std::error::Error`], which converts into a [`Failure::Runtime`].
///
/// A command does not have to return `Outcome` at all: `io::Result<()>`,
/// `anyhow::Result<()>` or `color_eyre::Result<()>` are accepted as they are
/// (ADR-012). Return `Outcome` when the command needs [`Failure::usage`].
pub type Outcome<T = ()> = Result<T, Failure>;

/// Everything a command is allowed to return, normalised to
/// [`Outcome<Success>`](Outcome).
///
/// There are exactly four shapes. The success side is closed — `()` or
/// [`Success`], nothing else — and the error side is generic over
/// `E: Into<Failure>`:
///
/// | Return type | Outcome |
/// |---|---|
/// | `()` | `Ok(Success::OK)` |
/// | [`Success`] | `Ok(that success)` |
/// | `Result<(), E>` where `E: Into<Failure>` | `Ok(Success::OK)` / `Err(e.into())` |
/// | `Result<Success, E>` where `E: Into<Failure>` | the success / `Err(e.into())` |
///
/// `E` is usually one of:
///
/// - [`Failure`] itself — [`Outcome`] and `Outcome<Success>` — which converts
///   reflexively, so a [`Failure::Usage`] keeps exit code `2`;
/// - any `std::error::Error + 'static` (`io::Error`, a `thiserror` enum, …);
/// - `anyhow::Error` and `eyre::Report` (so `anyhow::Result<()>` and
///   `color_eyre::Result<()>` are accepted as written);
/// - `Box<dyn Error>`, `String` and `&str`.
///
/// Everything but `Failure` becomes a [`Failure::Runtime`] (exit code `1`)
/// whose message is the error's `Display` followed by each
/// [`source`](std::error::Error::source) in turn, joined by `": "` —
/// `eyre!("root").wrap_err("mid").wrap_err("outer")` prints
/// `outer: mid: root`. See [`Failure`]'s `From` impl.
///
/// `Box<dyn Error + Send + Sync>` is **not** accepted: the standard library has
/// no conversion from it into `Box<dyn Error>`. Return `Box<dyn Error>`, or
/// map the error with [`Failure::runtime`].
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a command return type hicli accepts",
    label = "a `#[command]` must return `()`, `Success`, `Result<(), E>` or `Result<Success, E>`",
    note = "the four accepted shapes are `()`, `Success`, `Result<(), E>` and `Result<Success, E>`, where `E: Into<Failure>`",
    note = "`E` may be `Failure` (i.e. `hicli::Outcome`), any `std::error::Error + 'static`, `anyhow::Error`, `eyre::Report`, `Box<dyn Error>`, `String` or `&str`"
)]
pub trait IntoOutcome {
    /// Normalise this value to the outcome hicli's run path works with.
    fn into_outcome(self) -> Outcome<Success>;
}

impl IntoOutcome for () {
    /// A command that cannot fail and has nothing to report succeeds with `0`.
    fn into_outcome(self) -> Outcome<Success> {
        Ok(Success::OK)
    }
}

impl IntoOutcome for Success {
    /// An infallible command that still chooses its exit code.
    fn into_outcome(self) -> Outcome<Success> {
        Ok(self)
    }
}

/// Any error becomes a [`Failure::Runtime`] (exit code `1`), so `?` works on
/// it inside a command returning [`Outcome`].
///
/// The bound is "converts into `Box<dyn Error>`", not "implements `Error`":
/// that one bound covers every `std::error::Error + 'static`, `String` and
/// `&str` (std's own `From` impls), and `anyhow::Error` and `eyre::Report`
/// (which provide that conversion themselves) — without hicli depending on
/// either crate.
///
/// The message is the error's `Display`, then each
/// [`source`](std::error::Error::source) in turn, joined by `": "`
/// (`outer: mid: root`). It is never `{:?}`: that is developer output, with
/// backtraces and colour codes, not a message for the user.
impl<E> From<E> for Failure
where
    E: Into<Box<dyn std::error::Error + 'static>>,
{
    fn from(e: E) -> Self {
        let boxed: Box<dyn std::error::Error + 'static> = e.into();
        Failure::Runtime(chain(&*boxed))
    }
}

/// `e`'s `Display`, then each `source()` in turn, joined by `": "`.
fn chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut message = e.to_string();
    let mut next = e.source();
    while let Some(source) = next {
        message.push_str(": ");
        message.push_str(&source.to_string());
        next = source.source();
    }
    message
}

impl<E: Into<Failure>> IntoOutcome for Result<(), E> {
    /// The common case: `Ok(())` succeeds with `0`; an error converts into a
    /// [`Failure`] — a `Failure` as itself, anything else as
    /// [`Failure::Runtime`].
    fn into_outcome(self) -> Outcome<Success> {
        self.map(|()| Success::OK).map_err(Into::into)
    }
}

impl<E: Into<Failure>> IntoOutcome for Result<Success, E> {
    /// A fallible command that chooses its exit code; the error converts as in
    /// the `Result<(), E>` impl.
    fn into_outcome(self) -> Outcome<Success> {
        self.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_codes_follow_the_convention() {
        assert_eq!(Failure::usage("u").code(), 2);
        assert_eq!(Failure::runtime("r").code(), 1);
    }

    #[test]
    fn success_carries_its_code() {
        assert_eq!(Success::OK.get(), 0);
        assert_eq!(Success::code(7).get(), 7);
        assert_eq!(Success::code(255).get(), 255);
    }

    #[test]
    fn success_code_zero_is_ok() {
        // Replaces `Exit`'s `exit_code_zero_is_not_success`: a code of zero no
        // longer has to be normalised away, because it lives on the success
        // side, where it means exactly what it says.
        assert_eq!(Success::code(0).get(), 0);
        assert_eq!(Success::code(0), Success::OK);
    }

    #[test]
    fn failure_always_has_a_message() {
        assert_eq!(Failure::usage("misused").message(), "misused");
        assert_eq!(Failure::runtime("boom").message(), "boom");
    }

    #[test]
    fn failure_display_is_the_message() {
        assert_eq!(Failure::usage("misused").to_string(), "misused");
        assert_eq!(Failure::runtime("boom").to_string(), "boom");
    }

    #[test]
    fn unit_is_a_zero_success() {
        assert_eq!(().into_outcome(), Ok(Success::OK));
    }

    #[test]
    fn success_round_trips() {
        assert_eq!(Success::code(7).into_outcome(), Ok(Success::code(7)));
    }

    #[test]
    fn outcome_round_trips() {
        let ok: Outcome = Ok(());
        assert_eq!(ok.into_outcome(), Ok(Success::OK));
        let err: Outcome = Err(Failure::runtime("boom"));
        assert_eq!(err.into_outcome(), Err(Failure::runtime("boom")));
    }

    #[test]
    fn outcome_of_success_round_trips() {
        let ok: Outcome<Success> = Ok(Success::code(7));
        assert_eq!(ok.into_outcome(), Ok(Success::code(7)));
        let err: Outcome<Success> = Err(Failure::usage("misused"));
        assert_eq!(err.into_outcome(), Err(Failure::usage("misused")));
    }

    // --- ADR-012: errors convert through `Into<Failure>` ---------------------

    /// A two-level error written by hand: `outer` whose `source()` is `inner`.
    #[derive(Debug)]
    struct Outer(Inner);

    #[derive(Debug)]
    struct Inner;

    impl Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a")
        }
    }

    impl Display for Inner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("b")
        }
    }

    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    impl std::error::Error for Inner {}

    #[test]
    fn an_error_renders_its_source_chain() {
        let f = Failure::from(Outer(Inner));
        assert_eq!(f, Failure::Runtime("a: b".into()));
        assert_eq!(f.code(), 1);
    }

    #[test]
    fn an_io_error_is_a_runtime_failure() {
        let err: std::io::Result<()> = Err(std::io::Error::other("disk full"));
        let f = err.into_outcome().unwrap_err();
        assert_eq!(f, Failure::Runtime("disk full".into()));
        assert_eq!(f.code(), 1);
    }

    #[test]
    fn a_string_is_a_runtime_failure() {
        let err: Result<(), String> = Err("no such source".to_string());
        let f = err.into_outcome().unwrap_err();
        assert_eq!(f, Failure::Runtime("no such source".into()));
        assert_eq!(f.code(), 1);
    }

    #[test]
    fn a_usage_failure_keeps_code_two() {
        // `Failure` converts reflexively: it is not boxed and rewrapped as a
        // runtime error.
        let err = Err::<(), Failure>(Failure::usage("misused"));
        let f = err.into_outcome().unwrap_err();
        assert_eq!(f, Failure::usage("misused"));
        assert_eq!(f.code(), 2);
    }

    #[test]
    fn a_success_under_a_foreign_error_keeps_its_code() {
        let ok = Ok::<_, std::io::Error>(Success::code(7));
        assert_eq!(ok.into_outcome(), Ok(Success::code(7)));
    }

    #[test]
    fn question_mark_converts_inside_an_outcome() {
        fn read() -> Outcome {
            Err(std::io::Error::other("gone"))?;
            Ok(())
        }
        assert_eq!(read(), Err(Failure::Runtime("gone".into())));
    }
}
