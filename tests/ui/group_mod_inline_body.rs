//! A module with a body does not need `group_mod!` at all: that is the inline
//! form, and `#[group]` goes straight on it. The shim exists only because an
//! attribute cannot yet be written on a `mod` *declaration*.

use hicli::group_mod;

struct Ctx;

group_mod! {
    /// events and instances
    pub mod event {
        /// create an event
        #[hicli::command]
        pub fn create(_ctx: &super::Ctx) {}
    }
}

fn main() {}
