//! A group with no commands cannot be mounted, so the macro refuses it where it
//! is written — `hicli::group(…, vec![])`'s runtime panic is left for trees
//! built by hand.

use hicli::group;

struct Ctx;

/// a group nobody can enter
#[group]
mod empty {
    use super::*;

    /// Not marked, so not a command.
    pub fn helper(_ctx: &Ctx) {}
}

fn main() {}
