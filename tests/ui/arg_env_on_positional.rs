//! A positional has no name to read from the environment by.

use hicli::Arg;

/// The node to talk to
#[derive(Arg)]
#[arg(positional = "HOST", env = "PLAYR_NODE")]
pub struct Node(Option<String>);

fn main() {}
