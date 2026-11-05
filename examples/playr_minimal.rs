//! The README's Quick start: the first three `playr` commands and a global
//! option. `examples/playr.rs` grows it into the full program.
//!
//! Try: `cargo run --example playr_minimal -- --help`

// The argument newtypes are documented on their fields, not the type: bpaf
// turns a doc comment on a struct into a help heading (see the README).
#![allow(missing_docs)]

use std::fmt;
use std::process::ExitCode;
use std::str::FromStr;

use bpaf::Bpaf;
use hicli::{app, command, group, App, Arg};

/// Global options
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// The node to talk to
    #[bpaf(long, env("PLAYR_NODE"), argument("HOST"), fallback(Node::localhost()))]
    pub node: Node,
}

/// Built once, after parsing, and handed to every command.
pub struct Ctx {
    pub node: Node,
}

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Output(
    /// An audio output device
    #[bpaf(positional("NAME"))]
    pub Name,
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

// ---- the program's own types ---------------------------------------------

/// Lowercase letters, digits and `-`, such as `hdmi` or `bandcamp`.
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);

impl FromStr for Name {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if s.is_empty() || !s.chars().all(allowed) {
            return Err("a name uses only a-z, 0-9 and -".to_string());
        }
        Ok(Name(s.to_string()))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A host with an optional port: `pi.local` or `pi.local:7700`.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub host: String,
    pub port: Option<u16>,
}

impl Node {
    pub fn localhost() -> Self {
        Node {
            host: "localhost".to_string(),
            port: None,
        }
    }
}

impl FromStr for Node {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let (host, port) = match s.split_once(':') {
            Some((host, port)) => match port.parse() {
                Ok(port) => (host, Some(port)),
                Err(_) => return Err(format!("`{port}` is not a port number")),
            },
            None => (s, None),
        };
        let allowed = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.';
        if host.is_empty() || !host.chars().all(allowed) {
            return Err(format!("`{host}` is not a host name"));
        }
        Ok(Node {
            host: host.to_string(),
            port,
        })
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.port {
            Some(port) => write!(f, "{}:{port}", self.host),
            None => f.write_str(&self.host),
        }
    }
}

// The README's Quick start pastes this file's `--help`; this test is what keeps
// the pasted text true.
#[cfg(test)]
mod tests {
    use super::*;
    use hicli::cli;

    const TOP_HELP: &str = "\
Usage: playr [--node=HOST] COMMAND ...

Global options
        --node=HOST  The node to talk to
                     [env:PLAYR_NODE: N/A]

Available options:
    -h, --help       Prints help information

Available commands:
    ping             Check that the node answers
    playback         Playback control
";

    const PLAYBACK_HELP: &str = "\
Playback control

Usage: playr playback COMMAND ...

Available options:
    -h, --help  Prints help information

Available commands:
    start       Start playback from the queue
    stop        Stop playback
    set-output  Select an audio output device
";

    // The only test in this binary, so nothing else reads the environment while
    // it is changed: with `PLAYR_NODE` set, `--help` prints its value instead of
    // `N/A`, and the README shows the unset case. `bin_name("playr")` makes the
    // seam print what `run` prints for a binary named `playr`.
    #[test]
    fn help_matches_the_readme() {
        std::env::remove_var("PLAYR_NODE");
        let top = app()
            .bin_name("playr")
            .parse(cli! { --help })
            .err()
            .expect("--help short-circuits");
        assert_eq!(top.unwrap_stdout(), TOP_HELP);
        let playback = app()
            .bin_name("playr")
            .parse(cli! { playback --help })
            .err()
            .expect("--help short-circuits");
        assert_eq!(playback.unwrap_stdout(), PLAYBACK_HELP);
    }
}
