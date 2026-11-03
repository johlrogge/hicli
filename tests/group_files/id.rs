//! The body of the `id` group, declared in `tests/group_files.rs`.
//!
//! The context here is deliberately written as `&super::Ctx`. A group lifts the
//! first `&T` it finds into the *parent* scope, where the handle is written, so
//! a module-relative path is a compile error naming `#[group(ctx = …)]` — and
//! the declaration answers with exactly that.

use hicli::{command, Outcome};

/// create an identity
#[command]
pub fn create(ctx: &super::Ctx) -> Outcome {
    ctx.note("id-create");
    Ok(())
}
