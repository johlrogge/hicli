//! A completer is handed the value that **already parsed** — `&Id`, not the raw
//! word — and returns `Vec<(String, Option<String>)>`. This one takes `&str` and
//! returns bare strings, so it is rejected by `complete_with`'s bound rather
//! than by the derive: the derive has no way to know what `complete_ids` is.

use hicli::bpaf::Bpaf;
use hicli::Arg;

#[derive(Bpaf, Arg)]
#[arg(complete = complete_ids)]
struct Id(#[bpaf(positional("ID"))] String);

fn complete_ids(typed: &str) -> Vec<String> {
    vec![typed.to_string()]
}

fn main() {}
