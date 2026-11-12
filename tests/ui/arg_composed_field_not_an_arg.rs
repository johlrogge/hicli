//! A composed struct's field is an argument type, exactly as a command's
//! parameter is: a `String` field is `Arg`'s own E0277, on the field's type.

use hicli::Arg;

#[derive(Arg)]
pub struct Person {
    name: String,
}

fn main() {}
