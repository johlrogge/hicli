//! `env` without `long` or `short`: on a composed struct there is no option
//! for it to fall back for.

use hicli::Arg;

/// The node to talk to
#[derive(Arg)]
#[arg(long = "node", metavar = "HOST")]
pub struct NodeOpt(Option<String>);

#[derive(Arg)]
#[arg(env = "PLAYR_NODE")]
pub struct Globals {
    node: NodeOpt,
}

fn main() {}
