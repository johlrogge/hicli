//! `#[group]` reads `alias` by the same rules as `#[command]`: an alias equal
//! to the group's own name adds nothing.

use hicli::group;

struct Ctx;

/// events and instances
#[group(alias = "event")]
mod event {
    use super::*;

    /// create an event
    #[hicli::command]
    pub fn create(_ctx: &Ctx) {}
}

fn main() {}
