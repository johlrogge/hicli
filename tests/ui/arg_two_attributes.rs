//! One attribute, one answer. Two `#[arg(…)]` attributes used to let the last
//! one win in silence, leaving the source unable to say which completer the
//! type ended up with.

use hicli::Arg;

#[derive(Arg)]
#[arg(complete = dirs)]
#[arg(complete = files)]
struct Id(String);

fn main() {}
