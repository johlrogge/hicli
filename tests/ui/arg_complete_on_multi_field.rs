//! A struct with named fields and no `#[bpaf(…)]` is composed (ADR-019), and a
//! composed struct has no one value to complete: each field completes through
//! its own type. (The bpaf-mode rejection is `arg_complete_on_bpaf_multi_field`.)

use hicli::Arg;

#[derive(Arg)]
#[arg(complete = complete_refs)]
struct EventRef {
    dir: String,
    instance: String,
}

fn main() {}
