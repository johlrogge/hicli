//! A positional's metavar is the value of `positional = "…"`.

use hicli::Arg;

/// An id
#[derive(Arg)]
#[arg(positional, metavar = "ID")]
pub struct Id(String);

fn main() {}
