//! The same alias twice is a typo for two different ones.

use hicli::command;

struct Ctx;

/// show status
#[command(alias = "st", alias = "st")]
fn status(_ctx: &Ctx) {}

fn main() {}
