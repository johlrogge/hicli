//! Git-style external subcommands.
//!
//! `app foo ARGS…` execs an external binary `foo`, yet `app --help` and
//! completion still enumerate `foo` — because the [`ExternalCommand`] manifest is
//! the single source both the parser and help read. Discovery is never
//! implicit: there is no `$PATH` scan; a binary is advertised only if it was
//! registered by absolute path.
//!
//! An external's exit code is propagated as a *value*: the child's code comes
//! back as [`Success::code`], never `std::process::exit`. The library never
//! terminates the process on the consumer's behalf — destructors run, the
//! consumer's `main` gets its code, and the dispatch path is the same
//! in-process one every other command uses.
//!
//! A child that exits non-zero is a *success* here, not a [`Failure`]: the
//! external ran, it reported its own diagnostics, and hicli's only remaining
//! job is to carry the code out. Only hicli's own failure — being unable to
//! run the binary at all — is a [`Failure`], because that message is the one
//! nobody else will print.
//!
//! An `ExternalCommand` is a *manifest*, not a tree node: wrap it in
//! [`external`](crate::external) to get the [`Command`](crate::Command) that
//! mounts it into the tree.

use crate::outcome::{Failure, Success};
use bpaf::{any, Parser};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

/// A minimal argument schema for an external subcommand.
///
/// v1 carries only enough to enrich completion (names + help). Richer schemas
/// (types, arities, nested subcommands, value completion) are deferred until a
/// real consumer needs them.
#[derive(Debug, Clone, Default)]
pub struct ArgSchema {
    /// The external's arguments, in declaration order.
    pub args: Vec<ArgSpec>,
}

/// One argument in an [`ArgSchema`].
#[derive(Debug, Clone)]
pub struct ArgSpec {
    /// The argument's name (as shown in help/completion).
    pub name: String,
    /// A short help string.
    pub help: String,
}

/// A registered external subcommand.
///
/// Trust posture (baked in):
/// 1. No implicit `$PATH` scan, ever.
/// 2. Allowlisted by **absolute path**, supplied at registration.
/// 3. No implicit secret forwarding — the child receives argv only.
///
/// Construct with [`ExternalCommand::new`].
#[derive(Debug, Clone)]
pub struct ExternalCommand {
    pub(crate) name: &'static str,
    pub(crate) help: String,
    pub(crate) bin: PathBuf,
    pub(crate) schema: Option<ArgSchema>,
}

impl ExternalCommand {
    /// Register an external subcommand named `name`, backed by the binary at
    /// the absolute path `bin`.
    ///
    /// The path is intended to be absolute (the allowlist contract); a
    /// relative path is accepted but will be resolved by the OS at exec time
    /// against the current directory, which defeats the trust posture. Prefer
    /// an absolute path.
    pub fn new(name: &'static str, help: impl Into<String>, bin: impl Into<PathBuf>) -> Self {
        ExternalCommand {
            name,
            help: help.into(),
            bin: bin.into(),
            schema: None,
        }
    }

    /// Attach an [`ArgSchema`] used to enrich completion for this external.
    pub fn schema(mut self, schema: ArgSchema) -> Self {
        self.schema = Some(schema);
        self
    }

    /// The subcommand name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The one-line help from the manifest — what
    /// [`external`](crate::external) gives the [`Command`](crate::Command).
    pub(crate) fn help(&self) -> &str {
        self.help.as_str()
    }

    /// The external's **inner** parser: everything after the external's name,
    /// captured verbatim, as a runnable [`ExternalRun`].
    ///
    /// Inner on purpose — the `command(name)` wrapper, the `descr` and the
    /// `help` are applied by [`Command::mount`](crate::Command), so an external
    /// is mounted by exactly the same code as every other command.
    pub(crate) fn into_run_parser(self) -> impl Parser<ExternalRun> {
        let name = self.name;
        let bin = self.bin;
        let rest = any::<String, _, _>("ARGS", Some).many();
        rest.map(move |rest| ExternalRun {
            name,
            bin: bin.clone(),
            rest,
        })
    }
}

/// The child-process run behind an external subcommand: what the mounted
/// [`Invocation`](crate::Invocation)'s action calls.
#[derive(Debug)]
pub(crate) struct ExternalRun {
    name: &'static str,
    bin: PathBuf,
    rest: Vec<String>,
}

/// The child's exit code, with Unix signal deaths mapped to the shell's
/// `128 + signal` convention.
///
/// `ExitStatus::code()` is `None` when the child was killed by a signal; git
/// and every POSIX shell report `128 + N` there, and an external subcommand
/// must fail the same way it would have if the user had run the binary
/// directly. (`hicli` reports the code; it deliberately does **not**
/// re-raise the signal on itself — that is the consumer's `main`'s business.)
#[cfg(unix)]
fn code_of(status: ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.code().or_else(|| status.signal().map(|s| 128 + s))
}

/// The child's exit code. Non-Unix targets have no signal concept here.
#[cfg(not(unix))]
fn code_of(status: ExitStatus) -> Option<i32> {
    status.code()
}

/// Map a finished child's status onto hicli's exit convention.
///
/// The child ran, so whatever it reported is a [`Success`] carrying its code:
/// `0` for a clean exit, the child's own `n` otherwise, `128 + N` for a Unix
/// signal death. Nothing is printed for any of them — the child already wrote
/// its own diagnostics, and a `Success` has no message to add.
///
/// A code outside `0..=255` collapses to `1` rather than being truncated into a
/// different code — or, worse, into `0`. It is defensive only: `ExitStatus`
/// already masks a clean exit to the low 8 bits, and `128 + N` tops out at
/// `255` because `wait(2)` carries the signal number in 7 bits.
///
/// Split out of [`ExternalRun::run`] so the mapping is unit-testable without
/// spawning anything: an `ExitStatus` can be constructed directly on Unix via
/// `ExitStatusExt::from_raw`.
fn exit_from_status(name: &str, status: ExitStatus) -> Result<Success, Failure> {
    match code_of(status) {
        Some(n) => Ok(Success::code(u8::try_from(n).unwrap_or(1))),
        // Unreachable on Unix (`status()` waits for termination, and a signal
        // death is handled by `code_of`) — kept as a defensive runtime error
        // rather than a panic or a silent zero.
        None => Err(Failure::Runtime(format!(
            "external `{name}` terminated without an exit status"
        ))),
    }
}

impl ExternalRun {
    /// Exec the binary, wait, and report the child's outcome.
    ///
    /// This is the *only* place hicli spawns anything, and it happens on the
    /// run path alone: parsing, help and completion never reach it.
    pub(crate) fn run(self) -> Result<Success, Failure> {
        let status = Command::new(&self.bin)
            .args(&self.rest)
            .status()
            .map_err(|e| {
                Failure::Runtime(format!(
                    "external `{}`: cannot run {}: {e}",
                    self.name,
                    self.bin.display()
                ))
            })?;
        exit_from_status(self.name, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deliberately a path that cannot exist: nothing here may spawn anything.
    const BIN: &str = "/nonexistent/hicli-external-foo";

    /// Parse `args` through the external's command parser, in-process.
    ///
    /// The `command("foo")` wrapper is applied here because it is applied by
    /// `Command::mount` in production — these tests assert what the external
    /// captures *inside* its own subcommand, which is where `rest` comes from.
    fn run(args: &[&str]) -> Result<ExternalRun, bpaf::ParseFailure> {
        ExternalCommand::new("foo", "the foo plugin", BIN)
            .into_run_parser()
            .to_options()
            .command("foo")
            .to_options()
            .run_inner(args)
    }

    #[test]
    fn external_captures_rest_verbatim() {
        let r = run(&["foo", "a", "b"]).expect("parses");
        assert_eq!(r.name, "foo");
        assert_eq!(r.bin, PathBuf::from(BIN));
        assert_eq!(r.rest, ["a", "b"]);
    }

    #[test]
    fn external_captures_no_args() {
        let r = run(&["foo"]).expect("parses");
        assert!(r.rest.is_empty());
    }

    #[test]
    fn external_captures_flag_like_args() {
        // The git model: everything after the external's name is the child's,
        // including things that look like our own flags.
        let r = run(&["foo", "--verbose", "-n", "2"]).expect("parses");
        assert_eq!(r.rest, ["--verbose", "-n", "2"]);
    }

    #[test]
    fn external_captures_after_double_dash() {
        let r = run(&["foo", "--", "--verbose"]).expect("parses");
        // `--` is consumed by bpaf as the positional-only marker: it is NOT
        // retained in `rest`; only the words after it reach the child.
        assert_eq!(r.rest, ["--verbose"]);
    }

    /// `ExitStatus` has no portable constructor; on Unix it is the raw
    /// `wait(2)` status, so these build one directly — no process involved.
    #[cfg(unix)]
    fn exited(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code << 8)
    }

    #[cfg(unix)]
    fn signalled(sig: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(sig)
    }

    #[test]
    #[cfg(unix)]
    fn external_zero_status_is_success() {
        assert_eq!(exit_from_status("foo", exited(0)), Ok(Success::OK));
    }

    #[test]
    #[cfg(unix)]
    fn external_nonzero_status_becomes_a_silent_exit_code() {
        // The child ran and reported for itself, so this is a success carrying
        // the child's code — nothing of hicli's is printed.
        let s = exit_from_status("foo", exited(7)).expect("the child ran");
        assert_eq!(s, Success::code(7));
        assert_eq!(s.get(), 7);
    }

    #[test]
    #[cfg(unix)]
    fn external_killed_by_signal_is_128_plus_signal() {
        // SIGKILL: 128 + 9, matching what a shell reports for a signalled child.
        let s = exit_from_status("foo", signalled(9)).expect("the child ran");
        assert_eq!(s, Success::code(137));
        assert_eq!(s.get(), 137);
    }

    #[test]
    fn external_spawn_failure_is_a_runtime_error() {
        // BIN cannot exist, so the exec fails and nothing is ever executed.
        // This is the only test that touches `Command`, and it proves the
        // library returns an error instead of exiting the process.
        let r = ExternalRun {
            name: "foo",
            bin: PathBuf::from(BIN),
            rest: Vec::new(),
        };
        let e = r.run().expect_err("the binary cannot exist");
        assert_eq!(e.code(), 1);
        let m = e.message();
        assert!(m.contains("foo"), "message missing the name:\n{m}");
        assert!(m.contains(BIN), "message missing the path:\n{m}");
    }
}
