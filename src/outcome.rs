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

    /// Build a [`Failure::Runtime`] from any `Display` error, e.g.
    /// `something().map_err(Failure::runtime)?`.
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
/// about it and the `?` operator works either way.
pub type Outcome<T = ()> = Result<T, Failure>;

/// Everything a command is allowed to return, normalised to
/// [`Outcome<Success>`](Outcome).
///
/// The four impls are written out one by one, with **no blanket impl**: a
/// blanket `impl<T: Into<Success>>` (or over `Result<T, E>`) would make the set
/// of legal return types open-ended and its errors arrive as a wall of trait
/// bounds. Listing them keeps "what may a command return?" answerable by
/// reading this page:
///
/// | Return type | Outcome |
/// |---|---|
/// | `()` | `Ok(Success::OK)` |
/// | [`Success`] | `Ok(that success)` |
/// | [`Outcome`] (`Result<(), Failure>`) | `Ok(Success::OK)` / the failure |
/// | `Outcome<Success>` (`Result<Success, Failure>`) | itself |
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

impl IntoOutcome for Outcome {
    /// The common case: `?` on failure, `Ok(())` on success (exit code `0`).
    fn into_outcome(self) -> Outcome<Success> {
        self.map(|()| Success::OK)
    }
}

impl IntoOutcome for Outcome<Success> {
    /// Already an outcome — the identity impl, so the run path has exactly one
    /// shape to handle.
    fn into_outcome(self) -> Outcome<Success> {
        self
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
}
