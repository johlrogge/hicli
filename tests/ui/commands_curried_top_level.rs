//! A curried command needs its group's value to run, so it cannot be mounted
//! on its own: its handle is a `Curried<T, Ctx>`, not a `Command<Ctx>`.

use hicli::bpaf::Bpaf;
use hicli::{command, commands, Arg, Command};

struct Ctx;

/// A configured music source.
#[derive(Debug, Clone, Bpaf, Arg)]
struct SourceName(#[bpaf(positional("SOURCE-NAME"))] String);

/// sync a source
#[command(curried)]
fn sync(_s: SourceName, _ctx: &Ctx) {}

fn main() {
    let _: Vec<Command<Ctx>> = commands![sync];
}
