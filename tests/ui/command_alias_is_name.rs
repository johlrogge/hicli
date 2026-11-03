//! An alias equal to the command's own (derived) name adds nothing.

use hicli::command;

struct Ctx;

/// show status
#[command(alias = "status")]
fn status(_ctx: &Ctx) {}

fn main() {}
