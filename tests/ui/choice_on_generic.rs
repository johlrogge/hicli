//! A choice is concrete: `Choice::VARIANTS` is a constant list of values, so
//! there is nothing to fix `T` with.

use hicli::Choice;

#[derive(Choice)]
enum SortKey<T> {
    Bpm,
    Other(T),
}

fn main() {}
