//! A group that hands its commands an argument has no default command, so a
//! command cannot be both `curried` and `default`.

use hicli::bpaf::Bpaf;
use hicli::{command, Arg};

struct Ctx;

/// A configured music source.
#[derive(Debug, Clone, Bpaf, Arg)]
struct SourceName(#[bpaf(positional("SOURCE-NAME"))] String);

/// sync a source
#[command(curried, default)]
fn sync(_s: SourceName, _ctx: &Ctx) {}

fn main() {}
