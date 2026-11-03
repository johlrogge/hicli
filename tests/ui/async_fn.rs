//! A command runs to completion and reports an outcome; there is nothing to
//! await it.

use hicli::command;

struct Ctx;

/// show status
#[command]
async fn status(_ctx: &Ctx) {}

fn main() {}
