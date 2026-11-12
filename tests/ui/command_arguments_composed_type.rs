//! A composed struct has no row of its own, so it has no `ArgHelp`: an
//! `# Arguments` entry for it is refused instead of ignored.

use hicli::{command, Arg};

/// The event directory
#[derive(Debug, Clone, Arg)]
#[arg(positional = "DIR")]
pub struct Dir(pub String);

#[derive(Debug, Clone, Arg)]
pub struct EventRef {
    pub dir: Dir,
}

/// open an event
///
/// # Arguments
///
/// * `r` - The event to open
#[command]
fn open(r: EventRef) {
    let _ = r;
}

fn main() {}
