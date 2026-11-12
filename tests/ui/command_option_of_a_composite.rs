//! A struct of arguments whose fields are all optional succeeds with none of
//! them given, so `Option<Filters>` would always be `Some`. (A composite with a
//! required field is required, and `Option` of it is fine.)

use hicli::{command, Arg};

/// Only tracks by this artist
#[derive(Debug, Clone, Arg)]
#[arg(long = "artist", metavar = "NAME")]
pub struct Artist(pub Option<String>);

#[allow(missing_docs)]
#[derive(Debug, Clone, Arg)]
pub struct Filters {
    pub artist: Artist,
}

/// search tracks
#[command]
fn search(f: Option<Filters>) {
    let _ = f;
}

fn main() {}
