//! The generated closure calls the body by naming its arguments, so every
//! parameter needs a name — destructure inside the body instead.

use hicli::command;

struct Ctx;
struct Words(Vec<String>);

/// say something
#[command]
fn say(Words(_w): Words, _ctx: &Ctx) {}

fn main() {}
