//! A type spelled by `#[derive(Bpaf)]` keeps bpaf's help: it has no
//! `ArgHelp`, so an override on it is refused instead of ignored.

use hicli::bpaf::Bpaf;
use hicli::command;

#[allow(missing_docs)]
#[derive(Debug, Clone, Bpaf, hicli::Arg)]
pub struct Title(#[bpaf(positional("TITLE"))] pub String);

/// create an event
///
/// # Arguments
///
/// * `t` - The event's title
#[command]
fn create(t: Title) {
    let _ = t;
}

fn main() {}
