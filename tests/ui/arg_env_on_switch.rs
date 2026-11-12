//! A switch takes no value, so there is nothing for a variable to supply.

use hicli::Arg;

/// Print more
#[derive(Arg)]
#[arg(long = "verbose", env = "VERBOSE")]
pub struct Verbose(bool);

fn main() {}
