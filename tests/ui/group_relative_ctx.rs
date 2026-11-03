//! The context type is read inside the module and written outside it, in the
//! group's handle — so a path that is relative to the module means something
//! else, or nothing, one level up.

use hicli::group;

struct Ctx;

/// events and instances
#[group]
mod event {
    /// create an event
    #[hicli::command]
    pub fn create(_ctx: &super::Ctx) {}
}

fn main() {}
