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
    /// Returning [`Exit`] lets a verb signal a usage error (exit code 2) or a
    /// runtime error (exit code 1) without touching the process directly.
    fn run(self: Box<Self>, ctx: &Ctx) -> Result<(), Exit>;
}

/// A boxed [`Verb`] — the value flowing through the parser tree.
pub type BoxedVerb<Ctx> = Box<dyn Verb<Ctx>>;

/// The consumer's exit-code convention, decoupled from `bpaf`'s.
///
/// `bpaf` uses `1` for parse errors and `0` for help/version; many CLIs want
/// usage errors to be `2` instead. `verbandi` maps both parse failures and
/// [`Exit`] onto this convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// A usage error — the invocation was malformed. Exit code `2`.
    Usage(String),
    /// A runtime error — the command ran but failed. Exit code `1`.
    Runtime(String),
}

impl Exit {
    /// The process exit code for this outcome: `2` for usage, `1` for runtime.
    pub fn code(&self) -> i32 {
        match self {
            Exit::Usage(_) => 2,
            Exit::Runtime(_) => 1,
        }
    }

    /// The human-readable message carried by this outcome.
    pub fn message(&self) -> &str {
        match self {
            Exit::Usage(m) | Exit::Runtime(m) => m,
        }
    }

    /// Build a [`Exit::Runtime`] from any `Display` error, e.g.
    /// `something().map_err(Exit::runtime)?`.
    pub fn runtime<E: Display>(e: E) -> Self {
        Exit::Runtime(e.to_string())
    }
}

impl Display for Exit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}
