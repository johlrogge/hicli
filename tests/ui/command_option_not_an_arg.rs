//! `Option<String>`: the error names `String`, the type that is not an
//! argument type, not `Option<String>`.

use hicli::command;

/// greet someone
#[command]
fn greet(name: Option<String>) {
    let _ = name;
}

fn main() {}
