//! A switch takes no value, so there is no metavar to name.

use hicli::Arg;

/// Be quiet
#[derive(Arg)]
#[arg(long = "quiet", metavar = "Q")]
pub struct Quiet(bool);

fn main() {}
