//! The context is shared with every command, so one cannot borrow it mutably.

use hicli::command;

struct Ctx;

/// show status
#[command]
fn status(_ctx: &mut Ctx) {}

fn main() {}
