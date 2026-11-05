//! The `jobs` group: a nested file group as `x.rs`.

use hicli::{command, Outcome};

use crate::Ctx;

mod helper;

/// list the jobs
#[command]
pub fn list(ctx: &Ctx) -> Outcome {
    ctx.note(helper::FROM);
    Ok(())
}
