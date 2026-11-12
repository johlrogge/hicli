//! `Added` holds its `Option` inside, so it succeeds without `--added`:
//! `Option<Added>` would never be `None`. The item-level assertion beside the
//! handle refuses it, pointing at `Added` in the signature — and because it is
//! an item, `cargo check` reports it, not only `cargo build`.

use hicli::{command, Arg};

/// Only tracks added in the last DAYS days
#[derive(Debug, Clone, Arg)]
#[arg(long = "added", metavar = "DAYS")]
pub struct Added(pub Option<u32>);

/// search tracks
#[command]
fn search(a: Option<Added>) {
    let _ = a;
}

fn main() {}
