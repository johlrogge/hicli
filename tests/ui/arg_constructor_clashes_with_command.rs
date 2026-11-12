//! `#[derive(Arg)]` writes a snake_case function beside the type (`Upload` →
//! `upload()`), so a command named after the type in the same module clashes:
//! E0428, pointing at `#[derive(Arg)]`. Rename the type (`UploadArgs`) or the
//! function.

use hicli::{command, Arg};

/// A file to upload
#[derive(Debug, Clone, Arg)]
#[arg(positional = "FILE")]
pub struct Upload(pub String);

/// upload a file
#[command]
fn upload(file: Upload) {
    let _ = file;
}

fn main() {}
