//! `Option<Option<Limit>>`: the inner `Option<Limit>` is not an argument
//! type, and `Arg`'s note says where an `Option` goes.

use hicli::{command, Arg};

/// How many to take
#[derive(Debug, Clone, Arg)]
#[arg(long = "limit", metavar = "N")]
pub struct Limit(pub u32);

/// list tracks
#[command]
fn list(limit: Option<Option<Limit>>) {
    let _ = limit;
}

fn main() {}
