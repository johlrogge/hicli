//! A `Vec` inside is empty when absent, so `Option<Tags>` would always be
//! `Some`.

use hicli::{command, Arg};

/// Tags to add
#[derive(Debug, Clone, Arg)]
#[arg(positional = "TAG")]
pub struct Tags(pub Vec<String>);

/// tag a track
#[command]
fn tag(tags: Option<Tags>) {
    let _ = tags;
}

fn main() {}
