//! A curried parameter is parsed by its group, so its row is the group's.

use hicli::command;

/// A source
#[derive(hicli::Arg)]
#[arg(positional = "SOURCE")]
pub struct SourceName(pub String);

/// sync a source
///
/// # Arguments
///
/// * `source` - The source to sync
#[command(curried)]
pub fn sync(source: SourceName) {
    let _ = source;
}

fn main() {}
