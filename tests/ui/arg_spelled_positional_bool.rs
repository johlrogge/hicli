//! A `bool` is a switch, and a switch has a name.

use hicli::Arg;

/// Be quiet
#[derive(Arg)]
#[arg(positional)]
pub struct Quiet(bool);

fn main() {}
