//! Git-style external subcommands.
//!
//! `app foo ARGS…` execs an external binary `foo`, yet `app --help` and
//! completion still enumerate `foo` — because the [`ExternalVerb`] manifest is
//! the single source both the parser and help read. Discovery is never
//! implicit: there is no `$PATH` scan; a binary is advertised only if it was
//! registered by absolute path.

use crate::verb::{BoxedVerb, Exit, Verb};
use bpaf::{any, Parser};
use std::path::PathBuf;
use std::process::Command;

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

    /// Build the `bpaf` parser for this external: a `command` that captures the
    /// remainder verbatim and maps it to a runnable [`Verb`].
    pub(crate) fn into_parser<Ctx: 'static>(self) -> impl Parser<BoxedVerb<Ctx>> {
        let name = self.name;
        let bin = self.bin;
        let help = self.help;
        let rest = any::<String, _, _>("ARGS", Some).many();
        rest.to_options()
            .descr(help.as_str())
            .command(name)
            .help(help.as_str())
            .map(move |rest| {
                Box::new(ExternalRun {
                    name,
                    bin: bin.clone(),
                    rest,
                }) as BoxedVerb<Ctx>
            })
    }
}

/// The verb that execs an external binary and propagates its exit code.
struct ExternalRun {
    name: &'static str,
    bin: PathBuf,
    rest: Vec<String>,
}

impl<Ctx> Verb<Ctx> for ExternalRun {
    fn name(&self) -> &'static str {
        self.name
    }

    fn run(self: Box<Self>, _ctx: &Ctx) -> Result<(), Exit> {
        let status = Command::new(&self.bin)
            .args(&self.rest)
            .status()
            .map_err(Exit::runtime)?;
        match status.code() {
            Some(0) => Ok(()),
            // Propagate the child's exact exit code. `Exit` only encodes 1/2,
            // so anything else is forwarded directly; externals are a genuine
            // process boundary. A richer `Exit::Code` is deferred.
            Some(code) => std::process::exit(code),
            // Terminated by a signal (Unix): mirror the shell's 128 + signal
            // convention as best we can without unix-only APIs.
            None => Err(Exit::Runtime(format!(
                "external `{}` terminated by signal",
                self.name
            ))),
        }
    }
}
