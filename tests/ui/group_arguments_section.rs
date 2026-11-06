//! A group is a module: it has no parameters for `# Arguments` to name.

use hicli::group;

/// tracks
///
/// # Arguments
///
/// * `by` - Order
#[group]
mod tracks {
    /// list tracks
    #[hicli::command]
    pub fn list() {}
}

fn main() {}
