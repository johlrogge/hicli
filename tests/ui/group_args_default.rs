//! A group that takes an argument reads the word after its name as that
//! argument, so there is no "no command named" case for a default to fill.

use hicli::bpaf::Bpaf;
use hicli::{group, Arg};

struct Ctx;

/// A configured music source.
#[derive(Debug, Clone, Bpaf, Arg)]
struct SourceName(#[bpaf(positional("SOURCE-NAME"))] String);

/// music sources
#[group(args = SourceName)]
mod source {
    use super::*;

    /// list sources
    #[hicli::command(default)]
    pub fn list(_ctx: &Ctx) {}

    /// sync a source
    #[hicli::command(curried)]
    pub fn sync(_s: SourceName, _ctx: &Ctx) {}
}

fn main() {}
