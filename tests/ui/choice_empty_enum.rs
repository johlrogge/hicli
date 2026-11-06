//! A choice with no variants has nothing to type.

use hicli::Choice;

#[derive(Choice)]
enum SortKey {}

fn main() {}
