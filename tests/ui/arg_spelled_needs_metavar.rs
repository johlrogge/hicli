//! An option with a value names that value: `--port=PORT`.

use hicli::Arg;

/// The port
#[derive(Arg)]
#[arg(long = "port")]
pub struct Port(u16);

fn main() {}
