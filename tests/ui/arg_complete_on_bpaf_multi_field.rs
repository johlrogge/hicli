//! In bpaf mode a completer on the whole value is handed the value that
//! already parsed, so on a two-field struct it cannot tell which field is
//! being completed. The rejection names both right answers.

use hicli::bpaf::Bpaf;
use hicli::Arg;

#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_refs)]
struct EventRef {
    #[bpaf(positional("DIR"))]
    dir: String,
    #[bpaf(positional("INSTANCE"))]
    instance: String,
}

fn main() {}
