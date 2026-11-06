//! `#[choice(name = …)]` names one variant; on the enum it names nothing.

use hicli::Choice;

#[derive(Choice)]
#[choice(name = "sort-key")]
enum SortKey {
    Bpm,
}

fn main() {}
