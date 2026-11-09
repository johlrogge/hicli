//! A struct with several fields is one word parsed whole, so it needs a `FromStr`.

use hicli::Arg;

/// A reference
#[derive(Arg)]
#[arg(positional)]
pub struct Ref {
    pub a: String,
    pub b: String,
}

fn main() {}
