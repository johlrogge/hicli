//! A type is spelled once: by `#[arg(…)]` or by `#[derive(Bpaf)]`, not both.

use hicli::bpaf::Bpaf;
use hicli::Arg;

/// An id
#[derive(Bpaf, Arg)]
#[arg(positional = "ID")]
pub struct Id(#[bpaf(positional("ID"))] String);

fn main() {}
