//! A file `#[group]` inside an *inline* `#[group]`. rustc resolves
//! `mod audience;` inside `mod event { … }` against `event/`, but the child's
//! own `#[group]` can only see the declaring file — so the two would read
//! different files. The parent refuses it where it is written, at the child's
//! name, instead of leaving a confusing "cannot find the file" to the child.

use hicli::group;

/// events and instances
#[group]
pub mod event {
    use super::*;

    /// manage audiences
    #[group]
    pub mod audience;
}

fn main() {}
