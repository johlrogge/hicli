//! `hide` is a switch: saying it twice is a mistake, not emphasis.

use hicli::command;

struct Ctx;

/// show status
#[command(hide, hide)]
fn status(_ctx: &Ctx) {}

fn main() {}
