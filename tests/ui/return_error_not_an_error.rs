//! The error side of a command's `Result` must convert into a `Failure`. A type
//! that is only `Debug` is not an error, so hicli has no message to print for
//! it: implement `std::error::Error` (or `Display` + `Failure::runtime`).

use hicli::command;

struct Ctx;

#[derive(Debug)]
struct NotAnError;

/// try something
#[command]
fn attempt(_ctx: &Ctx) -> Result<(), NotAnError> {
    Err(NotAnError)
}

fn main() {}
