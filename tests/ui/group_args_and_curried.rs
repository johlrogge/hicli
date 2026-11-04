//! A group either takes an argument of its own (`args = T`) or receives its
//! parent's (`curried = T`) — both at once is deferred.

use hicli::bpaf::Bpaf;
use hicli::{group, Arg};

struct Ctx;

/// A configured music source.
#[derive(Debug, Clone, Bpaf, Arg)]
struct SourceName(#[bpaf(positional("SOURCE-NAME"))] String);

/// A download job.
#[derive(Debug, Clone, Bpaf, Arg)]
struct JobId(#[bpaf(positional("JOB-ID"))] String);

/// a source's jobs
#[group(args = JobId, curried = SourceName)]
mod jobs {
    use super::*;

    /// cancel a job
    #[hicli::command(curried)]
    pub fn cancel(_j: JobId, _ctx: &Ctx) {}
}

fn main() {}
