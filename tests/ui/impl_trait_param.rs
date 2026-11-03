//! `impl Trait` in argument position is a generic parameter in disguise, and
//! names no type to build a parser from.

use hicli::command;

struct Ctx;

/// create an event
#[command]
fn create(_name: impl Into<String>, _ctx: &Ctx) {}

fn main() {}
