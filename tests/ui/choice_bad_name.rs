//! A choice name is typed as a value: a word with a space would need quoting,
//! and one starting with `-` would be read as a flag.

use hicli::Choice;

#[derive(Choice)]
enum SortKey {
    #[choice(name = "--bpm")]
    Bpm,
}

fn main() {}
