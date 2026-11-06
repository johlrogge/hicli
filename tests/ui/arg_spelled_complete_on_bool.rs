//! A switch has no value, so there is nothing to complete.

use hicli::{Arg, Candidate};

fn yes(_: &str) -> Vec<Candidate> {
    Vec::new()
}

/// Be quiet
#[derive(Arg)]
#[arg(long = "quiet", complete = yes)]
pub struct Quiet(bool);

fn main() {}
