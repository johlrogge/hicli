//! A command's argument types are the parsers it calls, so they cannot be type
//! parameters.

use hicli::command;

struct Ctx;

/// show status
#[command]
fn status<T>(_arg: T, _ctx: &Ctx) {}

fn main() {}
