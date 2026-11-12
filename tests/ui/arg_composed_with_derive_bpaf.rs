//! `#[derive(Bpaf)]` beside a composed `#[derive(Arg)]`, with no `#[bpaf]`
//! attribute for the derive to see: both write the constructor `s()`, and
//! rustc reports the clash. Add a `#[bpaf(…)]` to keep bpaf mode, or drop
//! `#[derive(Bpaf)]`.

use hicli::bpaf::Bpaf;
use hicli::Arg;

/// A switch
#[derive(Debug, Clone, Arg)]
#[arg(long = "flag")]
pub struct Flag(bool);

// Only so that bpaf's own derive, which reads `a: Flag` as an argument, has
// nothing else to say: the clash is the point.
impl std::str::FromStr for Flag {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(Flag(s == "true"))
    }
}

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct S {
    a: Flag,
}

fn main() {}
