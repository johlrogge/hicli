//! A group runs at most one command when none is named: a second `default` is
//! refused at its own ident, naming the first.

use hicli::group;

struct Ctx;

/// search
#[group]
mod search {
    use super::*;

    /// search the library
    #[hicli::command(default)]
    pub fn library(_ctx: &Ctx) {}

    /// search the web
    #[hicli::command(default)]
    pub fn web(_ctx: &Ctx) {}
}

fn main() {}
