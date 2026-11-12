//! A curried command's first parameter is the value its group parsed, which
//! is always there.

use hicli::{command, Arg};

/// A source's name
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SOURCE")]
pub struct SourceName(pub String);

/// sync a source
#[command(curried)]
fn sync(s: Option<SourceName>) {
    let _ = s;
}

fn main() {}
