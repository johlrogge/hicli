//! A `curried` command receives its group's argument as its first owned
//! parameter. The context is not an owned parameter, so a command taking only
//! `&Ctx` has nothing to receive it in.

use hicli::command;

struct Ctx;

/// sync a source
#[command(curried)]
fn sync(_ctx: &Ctx) {}

fn main() {}
