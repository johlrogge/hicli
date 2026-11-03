//! The body of the `audience` group, declared in `tests/group_files/event.rs`.
//!
//! Third level, second file: `event.rs` is not a mod-rs file, so its own
//! `group_mod!` resolves `audience` into the directory named after it —
//! `tests/group_files/event/` — which is the same rule rustc would apply, and
//! the emitted `#[path]` says so out loud.

use hicli::{command, Outcome};

use crate::{Ctx, Title};

/// define an audience
#[command]
pub fn define(t: Title, ctx: &Ctx) -> Outcome {
    ctx.note(format!("define:{}", t.0));
    Ok(())
}
