//! The [`Verb`] trait and the [`Exit`] convention.
//!
//! One subcommand = one [`Verb`]. A verb knows its own name (for tests, help
//! and suggestions) and how to run itself against the consumer's shared
//! context `Ctx`.

use std::fmt::Display;

/// One subcommand.
///
/// `Ctx` is the consumer's shared context — whatever the application builds
/// once (from global options / the environment) and hands to every verb.
///
/// A verb is parsed as a value (via `bpaf`) and dispatched by composition, so
/// there is no central `match`: each verb owns its parser next to its handler.
pub trait Verb<Ctx> {
    /// The verb's name, as it appears on the command line.
    ///
    /// Used by help, completion, suggestions and — crucially — tests, which
    /// assert on the selected verb without running it.
    fn name(&self) -> &'static str;

    /// Run the verb against the shared context.
    ///
    /// Returning [`Exit`] lets a verb signal a usage error (exit code 2), a
    /// runtime error (exit code 1), or a specific code with no message
    /// ([`Exit::Code`]) without touching the process directly.
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit>;
}

/// A boxed [`Verb`] — the value flowing through the parser tree.
pub type BoxedVerb<Ctx> = Box<dyn Verb<Ctx>>;

/// The consumer's exit-code convention, decoupled from `bpaf`'s.
///
/// `bpaf` uses `1` for parse errors and `0` for help/version; many CLIs want
/// usage errors to be `2` instead. `verbandi` maps both parse failures and
/// [`Exit`] onto this convention.
///
/// `Usage` and `Runtime` carry a message that [`App::run`](crate::App::run)
/// prints to stderr. [`Exit::Code`] deliberately does not: it exists for a verb
/// that has *already* reported its own failure — notably an external
/// subcommand, whose child process wrote its own diagnostics — and only needs
/// the process exit code carried back out. Returning it as a value is what lets
/// `verbandi` propagate a child's exit code without calling
/// `std::process::exit` inside the library.
///
/// This enum is `#[non_exhaustive]`: match on it with a `_` arm, or prefer
/// [`Exit::code`] and [`Exit::message`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Exit {
    /// A usage error — the invocation was malformed. Exit code `2`.
    Usage(String),
    /// A runtime error — the command ran but failed. Exit code `1`.
    Runtime(String),
    /// Exit with a specific code, silently.
    ///
    /// Nothing is printed: the failure has already been reported by whoever
    /// produced the code. `0` is not a failure, so [`Exit::code`] normalises
    /// `Code(0)` to `1` rather than letting a failing verb report success.
    ///
    /// `n` should be in `1..=255`; POSIX reports only the low 8 bits of a
    /// process exit code.
    Code(i32),
}

impl Exit {
    /// The process exit code for this outcome: `2` for usage, `1` for runtime,
    /// the carried code for [`Exit::Code`].
    ///
    /// `Code(0)` is normalised to `1`: an `Err` can never mean success, and a
    /// silent `0` would make a failing command look like it passed.
    pub fn code(&self) -> i32 {
        match self {
            Exit::Usage(_) => 2,
            Exit::Runtime(_) => 1,
            Exit::Code(0) => 1,
            Exit::Code(n) => *n,
        }
    }

    /// The message `verbandi` should print to stderr for this outcome, if any.
    ///
    /// [`Exit::Code`] returns `None` — it is the silent variant, so
    /// [`App::run`](crate::App::run) prints nothing for it, not even a blank
    /// line.
    pub fn message(&self) -> Option<&str> {
        match self {
            Exit::Usage(m) | Exit::Runtime(m) => Some(m),
            Exit::Code(_) => None,
        }
    }

    /// Build an [`Exit::Runtime`] from any `Display` error, e.g.
    /// `something().map_err(Exit::runtime)?`.
    pub fn runtime<E: Display>(e: E) -> Self {
        Exit::Runtime(e.to_string())
    }
}

impl Display for Exit {
    /// A human rendering of the outcome, for logging.
    ///
    /// Note this is *not* [`Exit::message`]: an [`Exit::Code`] has no message to
    /// print but still renders as `exited with code N`, so a logged `Exit` is
    /// never blank. `verbandi` itself only ever prints [`Exit::message`].
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Exit::Usage(m) | Exit::Runtime(m) => f.write_str(m),
            Exit::Code(n) => write!(f, "exited with code {n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_the_convention() {
        assert_eq!(Exit::Usage("u".into()).code(), 2);
        assert_eq!(Exit::Runtime("r".into()).code(), 1);
        assert_eq!(Exit::Code(7).code(), 7);
    }

    #[test]
    fn exit_code_zero_is_not_success() {
        // An `Err` cannot report success: a silent `0` would make a failing
        // command look like it passed, so `Code(0)` is normalised to `1`.
        assert_eq!(Exit::Code(0).code(), 1);
    }

    #[test]
    fn exit_code_carries_no_message() {
        assert!(Exit::Code(7).message().is_none());
        assert_eq!(Exit::Usage("u".into()).message(), Some("u"));
        assert_eq!(Exit::Runtime("r".into()).message(), Some("r"));
    }

    #[test]
    fn exit_display_renders_a_code() {
        assert_eq!(Exit::Code(7).to_string(), "exited with code 7");
        assert_eq!(Exit::runtime("boom").to_string(), "boom");
    }
}
