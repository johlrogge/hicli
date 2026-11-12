//! A composed struct's `Option<T>` field is checked the same way as a
//! parameter: the assertion points at `Added` in the field.

use hicli::Arg;

/// Only tracks added in the last DAYS days
#[derive(Debug, Clone, Arg)]
#[arg(long = "added", metavar = "DAYS")]
pub struct Added(pub Option<u32>);

#[derive(Debug, Clone, Arg)]
pub struct Search {
    a: Option<Added>,
}

fn main() {}
