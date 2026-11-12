//! `#[arg(…)]` goes on a type, never on a field: in a composed struct a field
//! is an argument type.

use hicli::Arg;

#[derive(Arg)]
pub struct Filters {
    /// Only tracks by this artist
    #[arg(long, metavar = "NAME")]
    artist: Option<String>,
}

fn main() {}
