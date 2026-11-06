//! An argument is a positional or an option, never both.

use hicli::Arg;

/// An id
#[derive(Arg)]
#[arg(positional = "ID", long = "id")]
pub struct Id(String);

fn main() {}
