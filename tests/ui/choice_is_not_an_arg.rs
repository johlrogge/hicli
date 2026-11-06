//! A choice is a value, not an argument: it says what the words are, not where
//! they go on the command line. Used directly as a command's parameter, it is
//! the same "not a hicli argument type" error as a `String`.

use hicli::{command, Choice};

struct Ctx;

/// What to sort by.
#[derive(Choice)]
enum SortKey {
    Bpm,
    Title,
}

/// sort the listing
#[command]
fn sort(_key: SortKey, _ctx: &Ctx) {}

fn main() {}
