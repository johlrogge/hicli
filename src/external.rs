//! Git-style external subcommands.
//!
//! `app foo ARGS…` execs an external binary `foo`, yet `app --help` and
//! completion still enumerate `foo` — because the [`ExternalVerb`] manifest is
//! the single source both the parser and help read. Discovery is never
//! implicit: there is no `$PATH` scan; a binary is advertised only if it was
//! registered by absolute path.
//!
//! An external's exit code is propagated as a *value*: the child's code comes
//! back as [`Exit::Code`], never `std::process::exit`. The library never
//! terminates the process on the consumer's behalf — destructors run, the
//! consumer's `main` gets its code, and the dispatch path is the same
//! in-process one every other verb uses.

use crate::verb::{BoxedVerb, Exit, Verb};
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
/// Construct with [`ExternalVerb::new`].
#[derive(Debug, Clone)]
pub struct ExternalVerb {
    pub(crate) name: &'static str,
    pub(crate) help: String,
    pub(crate) bin: PathBuf,
    pub(crate) schema: Option<ArgSchema>,
}

impl ExternalVerb {
    /// Register an external subcommand named `name`, backed by the binary at
    /// the absolute path `bin`.
    ///
    /// The path is intended to be absolute (the allowlist contract); a
    /// relative path is accepted but will be resolved by the OS at exec time
    /// against the current directory, which defeats the trust posture. Prefer
    /// an absolute path.
    pub fn new(name: &'static str, help: impl Into<String>, bin: impl Into<PathBuf>) -> Self {
        ExternalVerb {
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

    /// The external's command parser, yielding the concrete [`ExternalRun`].
    ///
    /// Split out of [`ExternalVerb::into_parser`] so unit tests can inspect the
    /// captured `rest` without downcasting a `Box<dyn Verb<_>>`.
    fn into_run_parser(self) -> impl Parser<ExternalRun> {
        let name = self.name;
        let bin = self.bin;
        let help = self.help;
        let rest = any::<String, _, _>("ARGS", Some).many();
        rest.to_options()
            .descr(help.as_str())
            .command(name)
            .help(help.as_str())
            .map(move |rest| ExternalRun {
                name,
                bin: bin.clone(),
                rest,
            })
    }

    /// Build the `bpaf` parser for this external: a `command` that captures the
    /// remainder verbatim and maps it to a runnable [`Verb`].
    pub(crate) fn into_parser<Ctx: 'static>(self) -> impl Parser<BoxedVerb<Ctx>> {
        self.into_run_parser()
            .map(|r| Box::new(r) as BoxedVerb<Ctx>)
    }
}

/// The verb that execs an external binary and propagates its exit code.
#[derive(Debug)]
struct ExternalRun {
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
/// directly. (`verbandi` reports the code; it deliberately does **not**
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

/// Map a finished child's status onto verbandi's exit convention.
///
/// Split out of [`ExternalRun::run`] so the mapping is unit-testable without
/// spawning anything: an `ExitStatus` can be constructed directly on Unix via
/// `ExitStatusExt::from_raw`.
fn exit_from_status(name: &str, status: ExitStatus) -> Result<(), Exit> {
    match code_of(status) {
        Some(0) => Ok(()),
        // The child's exact code, carried back as a value. `Exit::Code` is
        // silent: the child already wrote its own diagnostics.
        Some(n) => Err(Exit::Code(n)),
        // Unreachable on Unix (`status()` waits for termination, and a signal
        // death is handled above) — kept as a defensive runtime error rather
        // than a panic or a silent zero.
        None => Err(Exit::Runtime(format!(
            "external `{name}` terminated without an exit status"
        ))),
    }
}

impl<Ctx> Verb<Ctx> for ExternalRun {
    fn name(&self) -> &'static str {
        self.name
    }

    fn run(self: Box<Self>, _ctx: &Ctx) -> Result<(), Exit> {
        let status = Command::new(&self.bin)
            .args(&self.rest)
            .status()
            .map_err(|e| {
                Exit::Runtime(format!(
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
    const BIN: &str = "/nonexistent/verbandi-external-foo";

    /// Parse `args` through the external's command parser, in-process.
    fn run(args: &[&str]) -> Result<ExternalRun, bpaf::ParseFailure> {
        ExternalVerb::new("foo", "the foo plugin", BIN)
            .into_run_parser()
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
        assert_eq!(exit_from_status("foo", exited(0)), Ok(()));
    }

    #[test]
    #[cfg(unix)]
    fn external_nonzero_status_becomes_a_silent_exit_code() {
        let e = exit_from_status("foo", exited(7)).expect_err("nonzero is a failure");
        assert_eq!(e, Exit::Code(7));
        assert_eq!(e.code(), 7);
        assert!(e.message().is_none());
    }

    #[test]
    #[cfg(unix)]
    fn external_killed_by_signal_is_128_plus_signal() {
        // SIGKILL: 128 + 9, matching what a shell reports for a signalled child.
        let e = exit_from_status("foo", signalled(9)).expect_err("a signal death is a failure");
        assert_eq!(e, Exit::Code(137));
        assert_eq!(e.code(), 137);
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
        let e = Verb::<()>::run(Box::new(r), &()).expect_err("the binary cannot exist");
        assert_eq!(e.code(), 1);
        let m = e.message().expect("a spawn failure is reported");
        assert!(m.contains("foo"), "message missing the name:\n{m}");
        assert!(m.contains(BIN), "message missing the path:\n{m}");
    }
}
