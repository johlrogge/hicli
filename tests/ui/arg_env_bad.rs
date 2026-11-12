//! `env` names a variable: an empty name cannot.

use hicli::Arg;

/// The node to talk to
#[derive(Arg)]
#[arg(long = "node", metavar = "HOST", env = "")]
pub struct NodeOpt(Option<String>);

fn main() {}
