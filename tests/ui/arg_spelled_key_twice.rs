//! Each key says one thing once.

use hicli::Arg;

/// Be quiet
#[derive(Arg)]
#[arg(long = "quiet", long = "silent")]
pub struct Quiet(bool);

fn main() {}
