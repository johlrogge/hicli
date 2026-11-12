//! A path type that is not an argument type: one error, over the whole path.

use hicli::command;

mod path {
    pub struct NotArg;
}

/// greet someone
#[command]
fn greet(name: path::NotArg) {
    let _ = name;
}

fn main() {}
