//! `#[cfg]` on a composed struct's field.

use hicli::Arg;

/// How many to take
#[derive(Debug, Clone, Arg)]
#[arg(long = "limit", metavar = "N")]
pub struct Limit(pub u32);

#[derive(Debug, Clone, Arg)]
pub struct Search {
    #[cfg(unix)]
    limit: Limit,
}

fn main() {}
