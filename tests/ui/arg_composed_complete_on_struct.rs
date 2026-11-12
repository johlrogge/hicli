//! A composed struct has no one value to complete: each field completes
//! through its own type.

use hicli::Arg;

/// The event directory
#[derive(Arg)]
#[arg(positional = "DIR")]
pub struct Dir(String);

#[derive(Arg)]
#[arg(complete = complete_refs)]
pub struct EventRef {
    dir: Dir,
}

fn main() {}
