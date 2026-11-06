//! A choice is a value; it becomes an argument only when `#[derive(Arg)]` with
//! `#[arg(positional|long|short …)]` says where it goes on the command line.
//! Without that, used directly as a command's parameter, it is the same "not a
//! hicli argument type" error as a `String`, whose notes name the spelled form.

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
