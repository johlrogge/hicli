//! A small end-to-end `hicli` CLI: living documentation and the demo.
//!
//! Try:
//! ```text
//! cargo run --example demo -- --help
//! cargo run --example demo -- status
//! cargo run --example demo -- event --help
//! cargo run --example demo -- event create ./dir "My Event"
//! cargo run --example demo -- event list
//! cargo run --example demo -- event ls          # an alias: same command
//! cargo run --example demo -- event create --help   # examples after the options
//! cargo run --example demo -- debug-ctx         # hidden: unlisted, still runs
//! cargo run --example demo -- --version
//! cargo run --example demo -- event audience --help
//! cargo run --example demo -- event audience define ./dir Friends --seal-roster
//! cargo run --example demo -- id create
//! cargo run --example demo -- event creat ./dir x   # typo -> suggestion, exit 2
//! cargo run --example demo -- event create ./dir    # missing NAME -> usage, exit 2
//! cargo run --example demo -- event create ./dir a b c d e f  # guard -> usage, exit 2
//! cargo run --example demo -- foo a b   # external, never on $PATH -> runtime, exit 1
//! cargo run --example demo -- --bpaf-complete-rev=1 ev   # what the shell asks on TAB
//! ```
//!
//! In this repo, `just demo --help` is the same thing with less typing.
//!
//! This is the **macro form**, which is the documented way to write a hicli
//! CLI: `#[derive(Bpaf, Arg)]` argument types, `#[command]` functions,
//! `#[group]` modules, one `app!`. Every one of those is sugar over
//! `hicli::command` / `hicli::group` / `App::new(…).commands(…)`, which stay
//! public — a command that outgrows the macro can be written out by hand.
//!
//! One thing this file cannot show, because it lives *inside* the hicli
//! package: a consumer crate needs its own `bpaf = "0.9"` dependency for
//! `#[derive(Bpaf)]`, because the derive expands to `::bpaf::…` paths.

use std::process::ExitCode;

use hicli::bpaf::Bpaf;
// One `use` for `command`, one for `group`, one for `Arg`: in every case the
// attribute/derive below and the `hicli::command` / `hicli::group` function or
// the `hicli::Arg` trait it expands into share a name across two namespaces.
use hicli::{app, command, group, App, Arg, ExternalCommand, Failure, Outcome, Success};

// ---------------------------------------------------------------------------
// Globals and context
// ---------------------------------------------------------------------------

// Globals are not a command's parameter, so they stay a plain `#[derive(Bpaf)]`
// struct and deliberately do **not** derive `Arg`: `App::new(globals())` takes
// the parser directly, and nothing ever asks this type how it parses itself.
// The struct's doc comment becomes a header in `--help`, so keep it to one line
// of prose a user would want to read.
/// Global options, accepted before or after the subcommand.
#[derive(Debug, Clone, Bpaf)]
struct Globals {
    /// Base directory for the demo's data.
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
    home: String,
}

/// The shared context handed to every command.
///
/// Built once by `mk_ctx`, *after* parsing. Anything secret belongs here, read
/// from the environment — never as a parser field, where `--help` would
/// advertise it.
struct Ctx {
    home: String,
}

/// Build the context from the globals. A failure here is a usage error (2).
fn mk_ctx(g: Globals) -> Result<Ctx, Failure> {
    if g.home.is_empty() {
        return Err(Failure::usage("--home needs a path"));
    }
    Ok(Ctx { home: g.home })
}

// ---------------------------------------------------------------------------
// Shared argument structs
//
// This is why `#[command]` has no argument attributes: an argument *group* is a
// type, written once and reused by every command that takes it. hicli never
// sees the spellings below — bpaf does.
// ---------------------------------------------------------------------------

// Shared by `event create` and `event audience define`: written once, helped
// once, tested once. An argument struct's own doc comment becomes a header in
// `app <cmd> --help`, so — as for `Globals` — it stays one line of prose.
//
// `Arg` is what makes it usable as a `#[command]` parameter: it says "this
// type's parser is `event_dir()`", once, here — instead of at every command
// that takes one. `#[arg(complete = dirs)]` hands the shell's own directory
// completion to every such command, with nothing written at any of them.
/// Where the event lives.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]
struct EventDir(
    /// the event directory
    #[bpaf(positional("DIR"))]
    String,
);

// A newtype with an invariant. The word limit is a bpaf `guard`, not an `if` in
// a command's body: it fails on the *parse* path, so it renders with usage and
// exits 2, and the command keeps only domain logic.
/// The event's display name, one to five words.
#[derive(Debug, Clone, Bpaf, Arg)]
struct EventName(
    /// the display name
    #[bpaf(
        positional("NAME"),
        some("a name is required"),
        guard(at_most_five_words, "a name is at most five words")
    )]
    Vec<String>,
);

impl EventName {
    /// The words as one string.
    fn joined(&self) -> String {
        self.0.join(" ")
    }
}

/// `guard` after `many`/`some` guards the whole `Vec`; before it, each element.
/// bpaf wants `fn(&T) -> bool`, hence the `&Vec` allow.
#[allow(clippy::ptr_arg)]
fn at_most_five_words(words: &Vec<String>) -> bool {
    words.len() <= 5
}

// ---------------------------------------------------------------------------
// The `event` group
// ---------------------------------------------------------------------------

// A group is a documented module, and **a child is a child because it is in
// it**: `create`, `list` and the nested `audience` group below are this group's
// commands, in that order, with no list to keep in step with them. The first
// line of the doc comment is the group's one-line help (and the heading of
// `demo event --help`); a `#[group]` without a doc comment is a compile error,
// exactly as for a command.
/// events and instances
#[group]
mod event {
    use super::*;

    /// create an event
    ///
    /// The event directory is created if it does not exist.
    ///
    /// # Examples
    ///
    /// ```sh
    /// demo event create ./summer "Summer Party"
    /// demo --home /srv event create ./board Board meeting
    /// ```
    // ^ the first line is the one-line help (the parent's listing *and* the
    //   heading of `--help`); the prose after it becomes the long description;
    //   the `# Examples` section becomes a footer, printed verbatim after the
    //   options. A command with no doc comment is a compile error — help is
    //   mandatory. Fence examples as ```sh: in a library crate rustdoc would
    //   try to compile an untagged block as Rust.
    #[command]
    pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
        // Each owned parameter is one argument type, parsed through its own
        // `Arg` impl (`<EventDir as Arg>::parser()`, `<EventName as
        // Arg>::parser()`) and composed in declaration order — which is the
        // order of the positionals on the command line. `ctx: &Ctx`, the one
        // reference parameter, is the context and fixes this command's `Ctx`.
        if dir.0.is_empty() {
            return Err(Failure::runtime("the event directory must not be empty"));
        }
        println!(
            "[home={}] created event at {} named {:?}",
            ctx.home,
            dir.0,
            name.joined()
        );
        Ok(())
    }

    // A command with no owned parameters takes no arguments at all, and a
    // one-line doc comment leaves it with no long description. `alias` is
    // another spelling: `demo event ls` runs this, its path is still
    // `["event", "list"]`, and `--help` lists only `list`.
    /// list events
    #[command(alias = "ls")]
    pub fn list(ctx: &Ctx) -> Outcome {
        println!("[home={}] no events yet", ctx.home);
        Ok(())
    }

    // A nested group is a nested module: `#[group]` inside a `#[group]`, and
    // the parent finds it the same way it finds a command. Its own context is
    // not written anywhere either — `define` takes `&Ctx`, so the whole tree
    // does.
    /// manage audiences
    ///
    /// An audience is a set of people invited together.
    #[group]
    pub mod audience {
        use super::*;

        /// Options for defining an audience.
        #[derive(Debug, Clone, Bpaf, Arg)]
        pub struct DefineOpts {
            /// freeze the roster once the audience is defined
            #[bpaf(long("seal-roster"), switch)]
            pub seal_roster: bool,
        }

        // `opts` is declared *first* because bpaf requires every positional to
        // sit at the right-hand end of the structure it is composed into — a
        // rule about the *parameter* order only: `--seal-roster` is still
        // accepted anywhere on the command line.
        /// define an audience
        ///
        /// The audience is sealed only if you say so.
        #[command]
        pub fn define(opts: DefineOpts, dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
            println!(
                "[home={}] audience {:?} in {} (roster sealed: {})",
                ctx.home,
                name.joined(),
                dir.0,
                opts.seal_roster
            );
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// The `id` group — a second module, and a second `create`
// ---------------------------------------------------------------------------

/// identity
#[group]
mod id {
    use super::*;

    // A second `create`, and it never collides with `event create`: one command
    // per module, named by its function. `invocation.path()` says which one it
    // is — `["id", "create"]`, never `["event", "create"]`.
    /// create an identity
    #[command]
    pub fn create(ctx: &Ctx) -> Outcome<Success> {
        println!("[home={}] created an identity", ctx.home);
        Ok(Success::OK)
    }
}

/// show demo status
#[command]
fn status(ctx: &Ctx) {
    println!("[home={}] ok", ctx.home);
}

// `hide` keeps a command out of `--help`, completion and `did you mean`, but
// typed exactly it still runs: `demo debug-ctx`.
/// print the context, for debugging
#[command(hide)]
fn debug_ctx(ctx: &Ctx) {
    println!("home = {:?}", ctx.home);
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// A git-style external subcommand: a separate binary mounted as
/// `demo foo ARGS…`.
///
/// Registered by **absolute path** — hicli never scans `$PATH` — and never
/// executed to find out what it is: `demo --help` and shell completion describe
/// it from this manifest alone. The path below deliberately does not exist, so
/// running `demo foo` reports a runtime failure (exit 1) while everything else
/// still lists it.
fn foo() -> ExternalCommand {
    ExternalCommand::new("foo", "an external subcommand", "/nonexistent/demo-foo")
}

/// The whole command tree. Named, so a test can drive the same one.
///
/// Each bare name is a registration: `commands!` reads it as the hidden handle
/// `#[command]` or `#[group]` wrote beside the item of that name. A group and a
/// command are indistinguishable here, which is the point — `event` is a
/// module, `status` is a function, and both are just names in a list.
///
/// The app's own description and version are set on the composition root:
/// `demo --help` opens with the description, and `demo --version` / `demo -V`
/// print the version and exit 0.
fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [event, id, status, debug_ctx, hicli::external(foo())],
    }
    .about("A small end-to-end hicli CLI.")
    .version(env!("CARGO_PKG_VERSION"))
}

fn main() -> ExitCode {
    // Codes are `u8`, so `run` hands back an `ExitCode` directly: `main` has
    // nothing left to convert and no out-of-range case to defend against.
    // hicli never calls `std::process::exit`.
    app().run(mk_ctx)
}
