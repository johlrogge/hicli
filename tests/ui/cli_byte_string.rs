//! An argv word is a string, so a byte string has no word to be.

use hicli::cli;

fn main() {
    let _: Vec<String> = cli! { name b"My Event" };
}
