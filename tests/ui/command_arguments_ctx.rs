//! The context is handed over, not parsed, so it has no help row to override.

use hicli::command;

struct Ctx;

/// list tracks
///
/// # Arguments
///
/// * `ctx` - The shared state
#[command]
fn list(ctx: &Ctx) {
    let _ = ctx;
}

fn main() {}
