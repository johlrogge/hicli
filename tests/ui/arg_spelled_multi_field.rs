//! Spelled mode spells one value; several are `#[derive(Bpaf)]`'s to compose.

use hicli::Arg;

/// A range
#[derive(Arg)]
#[arg(long = "range", metavar = "N")]
pub struct Range {
    from: u32,
    to: u32,
}

fn main() {}
