//! A command is a free function; a method has a receiver the command tree
//! cannot supply.

use hicli::command;

struct Ctx;

/// show status
#[command]
fn status(self, _ctx: &Ctx) {}

fn main() {}
