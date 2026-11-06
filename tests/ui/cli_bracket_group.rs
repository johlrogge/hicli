//! `cli!` reads words, and a bracketed group is not one: to Rust `[a]` is a
//! nested token tree, not text, so there is no word to take from it. Quoting
//! it says which word was meant.

use hicli::cli;

fn main() {
    let _: Vec<String> = cli! { tag [a] };
}
