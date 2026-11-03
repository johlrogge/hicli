//! `#[group]` on a file module whose file is not there. The message lists every
//! candidate it tried, so the fix is visible without knowing the Reference's
//! rules — and notes the one limitation of the file form, because "cannot find
//! the file" is also exactly what a file `#[group]` nested inside an inline
//! `mod` looks like.
//!
//! This is the *only* error: the attribute runs before rustc loads the file, so
//! the macro's diagnostic is the one the author sees, not rustc's own E0583.

use hicli::group;

/// events and instances
#[group]
pub mod event;

fn main() {}
