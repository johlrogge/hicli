//! A newtype is parsed by its own `FromStr`, else by its field's; with neither,
//! it cannot read a word.

use hicli::Arg;

pub struct Opaque;

/// Something opaque
#[derive(Arg)]
#[arg(positional = "X")]
pub struct Wrap(pub Opaque);

fn main() {}
