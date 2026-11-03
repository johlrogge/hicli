//! A completer on the whole value is handed the value that already parsed, so
//! on a two-field struct it cannot tell which field is being completed — and it
//! never fires while the other one is mid-word. The rejection names both right
//! answers rather than letting that compile and do nothing.

use hicli::Arg;

#[derive(Arg)]
#[arg(complete = complete_refs)]
struct EventRef {
    dir: String,
    instance: String,
}

fn main() {}
