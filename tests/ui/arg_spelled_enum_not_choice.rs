//! A spelled enum is its own value, so it has to be a `Choice`.

use hicli::Arg;

/// How fast
#[derive(Arg)]
#[arg(positional = "MODE")]
pub enum Mode {
    Fast,
    Slow,
}

fn main() {}
