//! A primitive in a command's signature is the whole reason `Arg` exists: hicli
//! ships no impl for `String`, so this is a compile error that names the newtype
//! to write instead of a CLI that silently invents a metavar.

use hicli::command;

struct Ctx;

/// create an event
#[command]
fn create(_name: String, _ctx: &Ctx) {}

fn main() {}
