//! An empty `long` would be the flag `--`, the end of options.

use hicli::Arg;

/// Be quiet
#[derive(Arg)]
#[arg(long = "")]
pub struct Quiet(bool);

fn main() {}
