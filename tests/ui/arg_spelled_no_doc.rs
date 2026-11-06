//! A spelled argument type's help row is its doc comment's first paragraph, so
//! a type with none has nothing to show.

use hicli::Arg;

#[derive(Arg)]
#[arg(positional = "ID")]
pub struct Id(String);

fn main() {}
