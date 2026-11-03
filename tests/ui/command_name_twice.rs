//! A command has one name; another spelling is an `alias`.

use hicli::command;

struct Ctx;

/// show status
#[command(name = "show", name = "st")]
fn status(_ctx: &Ctx) {}

fn main() {}
