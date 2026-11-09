//! A spelled parse error is printed after the argument's name, so it must be
//! `Display`.

use std::str::FromStr;

use hicli::Arg;

pub struct NotDisplay;

/// A name
#[derive(Arg)]
#[arg(positional = "NAME")]
pub struct Name(pub String);

impl FromStr for Name {
    type Err = NotDisplay;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Name(s.to_string()))
    }
}

fn main() {}
