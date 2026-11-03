//! A default command is never listed by name, so there is nothing to hide:
//! `hide` is refused beside `default`.

use hicli::command;

struct Ctx;

/// search the library
#[command(default, hide)]
fn search(_ctx: &Ctx) {}

fn main() {}
