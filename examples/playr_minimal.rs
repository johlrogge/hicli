//! The README's Quick start: the first three `playr` commands and a global
//! option. `examples/playr.rs` grows it into the full program.
//!
//! Try: `cargo run --example playr_minimal -- --help`

// The argument newtypes are documented on their fields, not the type: bpaf
// turns a doc comment on a struct into a help heading (see the README).
#![allow(missing_docs)]

use std::process::ExitCode;

use bpaf::Bpaf;
use hicli::{app, command, group, App, Arg};

/// Global options
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// The node to talk to
    #[bpaf(long, env("PLAYR_NODE"), argument("HOST"), fallback("localhost".into()))]
    pub node: String,
}

/// Built once, after parsing, and handed to every command.
pub struct Ctx {
    pub node: String,
}

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Output(
    /// An audio output device, as listed by `playr playback outputs`
    #[bpaf(positional("NAME"))]
    pub String,
);

/// Check that the node answers
#[command]
fn ping(ctx: &Ctx) {
    println!("pong from {}", ctx.node);
}

/// Playback control
#[group]
mod playback {
    use super::*;

    /// Start playback from the queue
    #[command]
    pub fn start(ctx: &Ctx) {
        println!("{}: playing", ctx.node);
    }

    /// Stop playback
    #[command]
    pub fn stop(ctx: &Ctx) {
        println!("{}: stopped", ctx.node);
    }

    /// Select an audio output device
    #[command]
    pub fn set_output(output: Output, ctx: &Ctx) {
        println!("{}: output is now {}", ctx.node, output.0);
    }
}

fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [ping, playback],
    }
}

fn main() -> ExitCode {
    app().run(|g| Ok(Ctx { node: g.node }))
}
