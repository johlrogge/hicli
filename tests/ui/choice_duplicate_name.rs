//! Two variants spelled the same word: `FromStr` could only ever return the
//! first. The error points at the second.

use hicli::Choice;

#[derive(Choice)]
enum SortKey {
    Artist,
    #[choice(name = "artist")]
    ByArtist,
}

fn main() {}
