//! `#[group]` goes on a module. A function is a `#[command]`.

use hicli::group;

struct Ctx;

/// show status
#[group]
fn status(_ctx: &Ctx) {}

fn main() {}
