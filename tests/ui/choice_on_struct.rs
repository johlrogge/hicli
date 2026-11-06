//! A choice is an enum: a struct has no fixed set of words to offer.

use hicli::Choice;

#[derive(Choice)]
struct SortKey(String);

fn main() {}
