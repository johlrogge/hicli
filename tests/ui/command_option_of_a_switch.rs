//! A switch is `false` when absent, so `Option<Yes>` would always be `Some`.

use hicli::{command, Arg};

/// Do not ask for confirmation
#[derive(Debug, Clone, Arg)]
#[arg(long = "yes", short = 'y')]
pub struct Yes(pub bool);

/// empty the archive
#[command]
fn empty(yes: Option<Yes>) {
    let _ = yes;
}

fn main() {}
