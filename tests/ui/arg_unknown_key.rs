//! `#[arg(…)]` has five keys — `positional`, `long`, `short`, `metavar` and
//! `complete` — and the message lists them all. The command's own name lives
//! elsewhere.

use hicli::Arg;

#[derive(Arg)]
#[arg(name = "ID")]
struct Id(String);

fn main() {}
