//! A group's help is its doc comment too (ADR-001), so a group without one has
//! no help.

use hicli::group;

struct Ctx;

#[group]
mod thing {
    use super::*;

    /// show status
    #[hicli::command]
    pub fn status(_ctx: &Ctx) {}
}

fn main() {}
