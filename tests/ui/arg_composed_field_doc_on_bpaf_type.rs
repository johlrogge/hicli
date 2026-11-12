//! A field's doc comment overrides its type's help, and a type spelled by
//! `#[derive(Bpaf)]` keeps bpaf's: the override is refused, not ignored.

use hicli::bpaf::Bpaf;
use hicli::Arg;

#[allow(missing_docs)]
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Title(#[bpaf(positional("TITLE"))] pub String);

#[derive(Debug, Clone, Arg)]
pub struct Event {
    /// The event's title
    title: Title,
}

fn main() {}
