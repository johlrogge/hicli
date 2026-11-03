//! Each owned parameter names the type whose bpaf constructor `#[command]`
//! calls; a tuple names no such type.

use hicli::command;

struct Ctx;

/// create an event
#[command]
fn create(_pair: (String, String), _ctx: &Ctx) {}

fn main() {}
