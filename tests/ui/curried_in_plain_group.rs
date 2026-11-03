//! A curried command needs a group that hands it an argument; a plain
//! `#[group]` has none to give, so the child is refused at its own ident.

use hicli::bpaf::Bpaf;
use hicli::{group, Arg};

struct Ctx;

/// A configured music source.
#[derive(Debug, Clone, Bpaf, Arg)]
struct SourceName(#[bpaf(positional("SOURCE-NAME"))] String);

/// music sources
#[group]
mod source {
    use super::*;

    /// list sources
    #[hicli::command]
    pub fn list(_ctx: &Ctx) {}

    /// sync a source
    #[hicli::command(curried)]
    pub fn sync(_s: SourceName, _ctx: &Ctx) {}
}

fn main() {}
