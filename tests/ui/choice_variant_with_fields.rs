//! A choice is one of a fixed set of words: a variant that carries a value is
//! not a word. The error points at the fields.

use hicli::Choice;

#[derive(Choice)]
enum SortKey {
    Bpm,
    Field(String),
}

fn main() {}
