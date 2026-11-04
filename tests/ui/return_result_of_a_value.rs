//! The success side of a command's `Result` is closed: `()` or `Success`. A
//! command that computes a `String` prints it; it does not return it.

use hicli::{command, Failure};

struct Ctx;

/// name something
#[command]
fn name(_ctx: &Ctx) -> Result<String, Failure> {
    Ok("x".into())
}

fn main() {}
