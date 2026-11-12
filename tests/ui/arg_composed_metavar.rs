//! `metavar` names an option's value; a composed struct has none of its own.

use hicli::Arg;

/// The event directory
#[derive(Arg)]
#[arg(positional = "DIR")]
pub struct Dir(String);

#[derive(Arg)]
#[arg(metavar = "REF")]
pub struct EventRef {
    dir: Dir,
}

fn main() {}
