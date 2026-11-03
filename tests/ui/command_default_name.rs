//! A default command runs when no command is named, so it has no name to
//! spell: `name` is refused beside `default`.

use hicli::command;

struct Ctx;

/// search the library
#[command(default, name = "find")]
fn search(_ctx: &Ctx) {}

fn main() {}
