//! The same CLI as `examples/demo.rs`, with its groups in **files**.
//!
//! Try:
//! ```text
//! cargo run --example files -- --help
//! cargo run --example files -- status
//! cargo run --example files -- event --help
//! cargo run --example files -- event create ./dir "My Event"
//! cargo run --example files -- event list
//! cargo run --example files -- event audience --help
//! cargo run --example files -- event audience define ./dir Friends --seal-roster
//! cargo run --example files -- event creat ./dir x   # typo -> suggestion, exit 2
//! cargo run --example files -- --bpaf-complete-rev=1 ev   # what the shell asks on TAB
//! ```
//!
//! In this repo, `just demo-files --help` is the same thing with less typing.
//!
//! `demo.rs` stays inline-only and this one stays file-only, so both forms are
//! living documentation and neither can rot unnoticed. Everything else is the
//! same: `#[derive(Bpaf, Arg)]` argument types, `#[command]` functions, one
//! `app!`.
//!
//! # What is different
//!
//! The group's *declaration* — its doc comment, and its name — lives here; its
//! *body* lives in `event.rs` beside this file. `#[group]` on the declaration
//! reads that file to find the group's children, and re-emits the declaration
//! unchanged — no `#[path]` — because rustc's own rules already find the same
//! bytes.
//!
//! This file is `examples/files/main.rs`, a mod-rs file by name, so `mod
//! event;` resolves beside it: `examples/files/event.rs`.

use std::process::ExitCode;

use hicli::bpaf::Bpaf;
use hicli::{app, command, group, App, Arg, Failure};

// ---------------------------------------------------------------------------
// Globals and context
// ---------------------------------------------------------------------------

// Not an `Arg`, deliberately: globals are composed at the top level by
// `App::new(globals())`, never as a command's parameter.
/// Global options, accepted before or after the subcommand.
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// Base directory for the demo's data.
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
    home: String,
}

/// The shared context handed to every command.
///
/// Built once by `mk_ctx`, *after* parsing. A command in another file reaches it
/// as `crate::Ctx`, which is a type name like any other — nothing about the
/// file form changes how context works.
pub struct Ctx {
    /// Where the demo keeps its data.
    pub home: String,
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
// They stay here, at the crate root, because more than one file's commands take
// them: an argument group is a type, written once and imported. `Arg` travels
// with the type, so `event.rs` imports the type and gets its parser — and its
// completion — without importing anything else.
// ---------------------------------------------------------------------------

/// Where the event lives.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]
pub struct EventDir(
    /// the event directory
    #[bpaf(positional("DIR"))]
    pub String,
);

/// The event's display name, one to five words.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct EventName(
    /// the display name
    #[bpaf(
        positional("NAME"),
        some("a name is required"),
        guard(at_most_five_words, "a name is at most five words")
    )]
    pub Vec<String>,
);

impl EventName {
    /// The words as one string.
    pub fn joined(&self) -> String {
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
// The tree
// ---------------------------------------------------------------------------

// The whole registration: a doc comment, `#[group]` and a `mod` declaration.
// The children are whatever is marked in `event.rs` — there is no list here to
// keep in step with that file, and moving a command in `--help` means moving a
// function.
/// events and instances
///
/// An event is a directory; an instance is a branch of it.
#[group]
pub mod event;

/// show demo status
#[command]
fn status(ctx: &Ctx) {
    println!("[home={}] ok", ctx.home);
}

/// The whole command tree. Named, so a test could drive the same one.
fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [event, status],
    }
}

fn main() -> ExitCode {
    app().run(mk_ctx)
}
