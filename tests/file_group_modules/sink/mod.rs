//! The `sink` group: a top-level file group as `x/mod.rs`.

use hicli::{command, Outcome};

use crate::Ctx;

mod helper;

/// drain everything
#[command]
pub fn drain(ctx: &Ctx) -> Outcome {
    ctx.note(helper::FROM);
    Ok(())
}
