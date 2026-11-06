//! `-` cannot be a short flag's letter: `--` already means something.

use hicli::Arg;

/// Be quiet
#[derive(Arg)]
#[arg(short = '-')]
pub struct Quiet(bool);

fn main() {}
