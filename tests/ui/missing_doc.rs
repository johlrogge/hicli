//! A command's help is its doc comment, so one without it has no help.

use hicli::command;

struct Ctx;

#[command]
fn status(_ctx: &Ctx) {}

fn main() {}
