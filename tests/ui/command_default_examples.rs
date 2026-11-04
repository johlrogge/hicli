//! A default command's examples belong on its group's help page, so its own
//! doc comment cannot carry an `# Examples` section.

use hicli::command;

struct Ctx;

/// search the library
///
/// # Examples
///
/// ```sh
/// demo search jazz
/// ```
#[command(default)]
fn search(_ctx: &Ctx) {}

fn main() {}
