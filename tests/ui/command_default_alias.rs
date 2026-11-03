//! A default command answers to no word, so it has no other word to answer to:
//! `alias` is refused beside `default`.

use hicli::command;

struct Ctx;

/// search the library
#[command(default, alias = "find")]
fn search(_ctx: &Ctx) {}

fn main() {}
