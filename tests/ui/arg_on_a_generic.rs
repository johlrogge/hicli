//! An argument type is concrete: `Arg::parser()` takes no parameters, so there
//! is nothing to fix `T` with.

use hicli::Arg;

#[derive(Arg)]
struct Id<T>(T);

fn main() {}
