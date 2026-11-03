//! The context is *the* `&T` parameter: a second one has no meaning.

use hicli::command;

struct Ctx;
struct Other;

/// show status
#[command]
fn status(_ctx: &Ctx, _other: &Other) {}

fn main() {}
