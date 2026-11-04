//! A curried command's first owned parameter *is* the group's argument, so its
//! type has to be the group's `args` type. `cancel` takes a `JobId` first in a
//! group that hands out a `SourceName`: the mismatch is reported at `cancel`,
//! where it is registered, not somewhere inside the group's handle.

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
#[group(args = SourceName)]
mod source {
    use super::*;

    /// sync a source
    #[hicli::command(curried)]
    pub fn sync(_s: SourceName, _ctx: &Ctx) {}

    /// cancel a job
    #[hicli::command(curried)]
    pub fn cancel(_j: JobId, _ctx: &Ctx) {}
}

fn main() {}
