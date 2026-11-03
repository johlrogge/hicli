//! The derived impl calls the type's snake_case `bpaf` constructor, and nothing
//! else writes it — so `#[derive(Arg)]` without `#[derive(Bpaf)]` is rustc's own
//! "cannot find function", spanned at the type's ident. That is the message to
//! read: the answer is to add the other derive.

use hicli::Arg;

#[derive(Arg)]
struct Id(String);

fn main() {}
