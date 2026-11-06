//! A spelled completer is handed the raw word, `&str` — not the parsed value
//! a bpaf-mode completer gets.

use hicli::{Arg, Candidate};

fn ports(_: &u16) -> Vec<Candidate> {
    Vec::new()
}

/// The port
#[derive(Arg)]
#[arg(long = "port", metavar = "PORT", complete = ports)]
pub struct Port(u16);

fn main() {}
