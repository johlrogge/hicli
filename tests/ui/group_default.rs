//! Only a command can be a default: a group always answers to its name, so
//! `default` is not a `#[group]` key.

use hicli::group;

struct Ctx;

/// search
#[group(default)]
mod search {
    use super::*;

    /// search the library
    #[hicli::command]
    pub fn library(_ctx: &Ctx) {}
}

fn main() {}
