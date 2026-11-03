//! A `#[group(curried = T)]` hands its parent's `T` on to *every* child, so
//! every child has to take it: an unmarked command is refused at its ident.

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

    /// a source's jobs
    #[group(curried = SourceName)]
    pub mod jobs {
        use super::*;

        /// cancel every job of a source
        #[hicli::command(curried)]
        pub fn cancel(_s: SourceName, _ctx: &Ctx) {}

        /// list every job
        #[hicli::command]
        pub fn list(_ctx: &Ctx) {}
    }
}

fn main() {}
