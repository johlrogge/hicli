//! `#[derive(Bpaf)]` beside a spelled `#[derive(Arg)]`, with no `#[bpaf]`
//! attribute for the derive to see: both write the constructor `id()`, and
//! rustc reports the clash.

use hicli::bpaf::Bpaf;
use hicli::Arg;

/// An id
#[derive(Bpaf, Arg)]
#[arg(positional = "ID")]
pub struct Id(String);

fn main() {}
