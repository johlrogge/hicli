//! Each parameter has one help: two entries for it would leave one unread.

use hicli::command;

/// Sort the listing by this field
#[derive(hicli::Arg)]
#[arg(long = "sort", metavar = "FIELD")]
pub struct SortOpt(pub Option<String>);

/// list tracks
///
/// # Arguments
///
/// * `by` - Order for this listing
/// * `by` - Or this one
#[command]
fn list(by: SortOpt) {
    let _ = by;
}

fn main() {}
