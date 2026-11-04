//! A command's return type must be one of `IntoOutcome`'s four shapes. An `i32`
//! is not a code — `Success::code(n)` is — so this is a compile error that
//! lists what *is* accepted.

use hicli::command;

struct Ctx;

/// count something
#[command]
fn count(_ctx: &Ctx) -> i32 {
    0
}

fn main() {}
