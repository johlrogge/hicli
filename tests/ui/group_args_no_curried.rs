//! A group that takes `args = T` parses a `T` for its curried children. With no
//! child marked `curried`, nobody would receive it — refused at the type.

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
    #[hicli::command]
    pub fn list(_ctx: &Ctx) {}
}

fn main() {}
