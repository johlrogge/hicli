//! A lifetime is a generic parameter too: the handle `#[command]` writes has no
//! place to put one.

use hicli::command;

struct Ctx;

/// show status
#[command]
fn status<'a>(_ctx: &'a Ctx) {}

fn main() {}
