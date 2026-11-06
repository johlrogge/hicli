//! An `# Arguments` entry names a parameter of the function it documents.

use hicli::command;

/// Sort the listing by this field
#[derive(hicli::Arg)]
#[arg(long = "sort", metavar = "FIELD")]
pub struct SortOpt(pub Option<String>);

/// list tracks
///
/// # Arguments
///
/// * `bye` - Order for this listing
#[command]
fn list(by: SortOpt) {
    let _ = by;
}

fn main() {}
