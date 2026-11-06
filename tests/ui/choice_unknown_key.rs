//! `#[choice(...)]` takes one key, `name`: help lives in the variant's doc
//! comment, and there are no aliases.

use hicli::Choice;

#[derive(Choice)]
enum SortKey {
    #[choice(alias = "b")]
    Bpm,
}

fn main() {}
