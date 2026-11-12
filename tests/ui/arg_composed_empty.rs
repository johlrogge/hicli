//! A composed struct with no fields has nothing to parse.

use hicli::Arg;

#[derive(Arg)]
pub struct Nothing {}

fn main() {}
