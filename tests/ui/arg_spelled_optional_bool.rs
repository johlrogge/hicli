//! A switch is already `false` when absent: `Option<bool>` says nothing more.

use hicli::Arg;

/// Be quiet
#[derive(Arg)]
#[arg(long = "quiet")]
pub struct Quiet(Option<bool>);

fn main() {}
