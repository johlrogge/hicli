//! The `deep` group: a nested file group as `x/mod.rs`.

use hicli::{command, Outcome};

use crate::Ctx;

mod helper;

/// dive in
#[command]
pub fn dive(ctx: &Ctx) -> Outcome {
    ctx.note(helper::FROM);
    Ok(())
}
