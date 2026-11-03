//! An alias spells a command, not a flag.

use hicli::command;

struct Ctx;

/// show status
#[command(alias = "-s")]
fn status(_ctx: &Ctx) {}

fn main() {}
