//! On a spelled type the spelling is the type's: a field's `#[arg(…)]` used to
//! be ignored, and is refused.

use hicli::Arg;

/// The port
#[derive(Arg)]
#[arg(long, metavar = "PORT")]
pub struct Port {
    #[arg(long = "p")]
    port: u16,
}

fn main() {}
