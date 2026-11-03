//! `ctx = <Type>` is the only argument `#[group]` takes. A group's name is its
//! module's ident — there is no `name = "…"` override.

use hicli::group;

struct Ctx;

/// events and instances
#[group(name = "events")]
mod event {
    use super::*;

    /// create an event
    #[hicli::command]
    pub fn create(_ctx: &Ctx) {}
}

fn main() {}
