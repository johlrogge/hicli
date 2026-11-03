//! An examples heading promises examples: an empty section is refused.

use hicli::command;

struct Ctx;

/// show status
///
/// # Examples
///
/// ```sh
/// ```
#[command]
fn status(_ctx: &Ctx) {}

fn main() {}
