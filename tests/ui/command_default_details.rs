//! A default command is shown on its group's help page, under its one-line
//! help — it has no page of its own for longer prose to go on.

use hicli::command;

struct Ctx;

/// search the library
///
/// Matches titles, artists and albums.
#[command(default)]
fn search(_ctx: &Ctx) {}

fn main() {}
