//! A group takes at most one argument (`args = T`): a second is refused.

use hicli::bpaf::Bpaf;
use hicli::{group, Arg};

struct Ctx;

/// A configured music source.
#[derive(Debug, Clone, Bpaf, Arg)]
struct SourceName(#[bpaf(positional("SOURCE-NAME"))] String);

/// A download job.
#[derive(Debug, Clone, Bpaf, Arg)]
struct JobId(#[bpaf(positional("JOB-ID"))] String);

/// music sources
#[group(args = SourceName, args = JobId)]
mod source {
    use super::*;

    /// sync a source
    #[hicli::command(curried)]
    pub fn sync(_s: SourceName, _ctx: &Ctx) {}
}

fn main() {}
