//! An empty alias is a word nobody can type.

use hicli::command;

struct Ctx;

/// show status
#[command(alias = "")]
fn status(_ctx: &Ctx) {}

fn main() {}
