//! Under `# Arguments`, every line is an entry or an indented continuation.

use hicli::command;

/// Sort the listing by this field
#[derive(hicli::Arg)]
#[arg(long = "sort", metavar = "FIELD")]
pub struct SortOpt(pub Option<String>);

/// list tracks
///
/// # Arguments
///
/// `by` orders the listing.
#[command]
fn list(by: SortOpt) {
    let _ = by;
}

fn main() {}
