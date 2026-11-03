//! `complete` is the only thing `#[arg(…)]` has to say. The metavar, the
//! positional-vs-option question and the command's own name all live elsewhere.

use hicli::Arg;

#[derive(Arg)]
#[arg(name = "ID")]
struct Id(String);

fn main() {}
