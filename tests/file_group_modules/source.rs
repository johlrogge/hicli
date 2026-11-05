//! The `source` group: a top-level file group with a plain submodule and two
//! nested file groups, one as `jobs.rs` and one as `deep/mod.rs`.

use hicli::{command, group, Outcome};

use crate::Ctx;

mod helper;

/// queued jobs
#[group]
pub mod jobs;

/// a directory module
#[group]
pub mod deep;

/// pull something in
#[command]
pub fn pull(ctx: &Ctx) -> Outcome {
    ctx.note(helper::FROM);
    Ok(())
}
