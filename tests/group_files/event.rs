//! The body of the `event` group, declared in `tests/group_files.rs`.
//!
//! An ordinary Rust file: the group's own help lives on the *declaration*, so
//! what is left here is the commands. Note that both nesting forms appear —
//! `instance` is an inline `#[group]`, `audience` is another `group_mod!` — and
//! that the declaring file for `audience` is *this* file, so it resolves into
//! `tests/group_files/event/`.

use hicli::{command, group, group_mod, Outcome};

// The type is the whole import: its `Arg` impl carries the parser with it.
use crate::{Ctx, Title};

/// create an event
#[command]
pub fn create(t: Title, ctx: &Ctx) -> Outcome {
    ctx.note(format!("create:{}", t.0));
    Ok(())
}

// `any()` is never true, so this command is not compiled — and the `push` in
// the handle, which carries the same `#[cfg]`, is not compiled either.
/// show the roster
#[cfg(any())]
#[command]
pub fn roster(ctx: &Ctx) -> Outcome {
    ctx.note("roster");
    Ok(())
}

/// manage instances
///
/// An instance is a branch of an event.
#[group]
pub mod instance {
    use super::*;

    /// branch an instance
    #[command]
    pub fn branch(t: Title, ctx: &Ctx) -> Outcome {
        ctx.note(format!("branch:{}", t.0));
        Ok(())
    }
}

group_mod! {
    /// manage audiences
    ///
    /// An audience is a set of people invited together.
    pub mod audience;
}
