//! A doc comment with no text in it is the same failure as no doc comment:
//! there is no first line to be the group's help. This was a runtime panic
//! until `#[group]` could read the text.

use hicli::group;

struct Ctx;

///
///
#[group]
mod thing {
    use super::*;

    /// show status
    #[hicli::command]
    pub fn status(_ctx: &Ctx) {}
}

fn main() {}
