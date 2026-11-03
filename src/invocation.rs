//! The parsed command: [`Invocation`].
//!
//! One subcommand = one [`Invocation`] — but an `Invocation` is not something a
//! consumer writes. It is what falls out of the parser: the selected command,
//! with its arguments already bound, waiting only for the shared context.
//!
//! There is no `Invocation` *trait*. An invocation is a struct carrying its
//! [`path`](Invocation::path) through the command tree and one `FnOnce`, so a
//! test can ask which command was selected without running it, and running it is
//! a single call.

use crate::outcome::{Failure, Success};

/// A command's bound body: everything but the context is already captured.
type Action<Ctx> = Box<dyn FnOnce(&Ctx) -> Result<Success, Failure>>;

/// A parsed subcommand, ready to run.
///
/// `Ctx` is the consumer's shared context — whatever the application builds
/// once (from global options / the environment) and hands to every command.
///
/// An invocation is produced by the parser, so dispatch happens by composition
/// and there is no central `match`: the command that matched is the one holding
/// the closure that will run.
pub struct Invocation<Ctx> {
    /// The full path from the root, e.g. `["event", "create"]`. Never empty:
    /// it is built by [`Command::mount`](crate::Command) from the names on the
    /// way down.
    path: Vec<&'static str>,
    /// The bound command body. `FnOnce`, because a parsed command runs once.
    action: Action<Ctx>,
}

impl<Ctx> Invocation<Ctx> {
    /// Build an invocation mounted at `path`, running `action`.
    ///
    /// Crate-private: an invocation exists only as the result of a parse, and
    /// its path is assigned by the fold that mounts the command tree — a
    /// consumer that could build one by hand could give it a path that is not in
    /// the tree.
    pub(crate) fn new(
        path: Vec<&'static str>,
        action: impl FnOnce(&Ctx) -> Result<Success, Failure> + 'static,
    ) -> Self {
        debug_assert!(!path.is_empty(), "a command's path names at least itself");
        Invocation {
            path,
            action: Box::new(action),
        }
    }

    /// The command's own name — the last segment of its
    /// [`path`](Invocation::path).
    ///
    /// Enough for a flat CLI; use `path()` when two commands in different
    /// groups share a name.
    pub fn name(&self) -> &'static str {
        self.path
            .last()
            .copied()
            .expect("a command's path names at least itself")
    }

    /// The full path from the root, e.g. `["event", "create"]`.
    ///
    /// This is what distinguishes `event create` from `id create` in a test,
    /// without running either.
    pub fn path(&self) -> &[&'static str] {
        &self.path
    }

    /// Run the command against the shared context.
    ///
    /// Returning [`Success`] finishes with a code — `0` for
    /// [`Success::OK`](crate::Success::OK), or a code of the command's choosing,
    /// printed as nothing at all. Returning [`Failure`] signals a usage error
    /// (exit code `2`) or a runtime error (exit code `1`), with a message
    /// hicli prints to stderr. Either way the code is carried out as a
    /// value; the process is never exited from inside the library.
    pub fn run(self, ctx: &Ctx) -> Result<Success, Failure> {
        (self.action)(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_the_last_path_segment() {
        let i = Invocation::new(vec!["event", "create"], |_: &()| Ok(Success::OK));
        assert_eq!(i.name(), "create");
        assert_eq!(i.path(), ["event", "create"]);
    }

    #[test]
    fn run_calls_the_bound_action_with_the_context() {
        let i = Invocation::new(vec!["greet"], |ctx: &String| {
            Ok(Success::code(u8::try_from(ctx.len()).unwrap_or(255)))
        });
        assert_eq!(i.run(&"abc".to_string()), Ok(Success::code(3)));
    }
}
