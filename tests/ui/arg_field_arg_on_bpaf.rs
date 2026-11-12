//! In bpaf mode a field is spelled by `#[bpaf(…)]`: a field's `#[arg(…)]` used
//! to be ignored, and is refused.

use hicli::bpaf::Bpaf;
use hicli::Arg;

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Filters {
    /// Only tracks by this artist
    #[bpaf(long, argument("NAME"))]
    #[arg(long, metavar = "NAME")]
    artist: Option<String>,
}

fn main() {}
