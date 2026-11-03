//! `#[command]` has one argument, and no argument-level attributes at all.

use hicli::command;

struct Ctx;

/// show status
#[command(help = "show status")]
fn status(_ctx: &Ctx) {}

fn main() {}
