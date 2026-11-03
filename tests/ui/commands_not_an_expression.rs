//! `commands!` has exactly one rule — a bare path is a handle, anything else is
//! an expression — so an *item* in the list matches neither half of it.

use hicli::{commands, Command};

struct Ctx;

fn main() {
    let _: Vec<Command<Ctx>> = commands![struct Foo;];
}
