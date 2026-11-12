//! `Option<path::NotArg>`: the error names the inner type, at the inner type.

use hicli::command;

mod path {
    pub struct NotArg;
}

/// greet someone
#[command]
fn greet(name: Option<path::NotArg>) {
    let _ = name;
}

fn main() {}
