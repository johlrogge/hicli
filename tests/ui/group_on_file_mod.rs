//! An attribute on a `mod` declaration — `#[group] pub mod event;` — is
//! feature-gated until Rust 1.99, and the toolchain is pinned below that.
//!
//! The gate (E0658) is the actionable error and comes first; rustc reports it
//! and then expands the attribute anyway, so the file form's own diagnostic
//! follows. That second message is the 1.99 behaviour, recorded here in
//! advance: `#[group]` on a file module routes to the very code path
//! `group_mod!` uses, and this file has no `event.rs` beside it. When the pin
//! moves past 1.99 the first error disappears and the second is all that is
//! left — which is the same error the shim gives today.
//!
//! Until then the file form is written `group_mod! { /// help  pub mod x; }`.

use hicli::group;

/// events and instances
#[group]
pub mod event;

fn main() {}
