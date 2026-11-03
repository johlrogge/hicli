//! An alias is one argv word; a space would make it two.

use hicli::command;

struct Ctx;

/// show status
#[command(alias = "show status")]
fn status(_ctx: &Ctx) {}

fn main() {}
