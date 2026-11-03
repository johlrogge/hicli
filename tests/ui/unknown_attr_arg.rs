//! `#[command]` takes `name`, `alias`, `hide`, `curried` and `default` — help is
//! the doc comment, never an attribute key.

use hicli::command;

struct Ctx;

/// show status
#[command(help = "show status")]
fn status(_ctx: &Ctx) {}

fn main() {}
