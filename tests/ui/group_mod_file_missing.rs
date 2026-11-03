//! A declaration whose file is not there. The message lists every candidate it
//! tried, so the fix is visible without knowing the Reference's rules — and
//! notes the one limitation of the shim, because "cannot find the file" is also
//! exactly what a `group_mod!` nested inside an inline `mod` looks like.

use hicli::group_mod;

group_mod! {
    /// events and instances
    pub mod event;
}

fn main() {}
