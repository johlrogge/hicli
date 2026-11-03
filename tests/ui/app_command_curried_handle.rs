//! A curried command's handle builds a `Curried<T, Ctx>`, which is deliberately
//! not `IntoCommand`: anything that mounts a command through `IntoCommand`
//! (here `App::command`) refuses it, with a note saying where it belongs.
//!
//! Only the *built* node reaches the note. Passing the handle itself
//! (`.command(__hicli_sync)`) matches the blanket `FnOnce() -> Command<Ctx>`
//! impl and fails on its return type (E0271) instead, without the note; and
//! `commands![sync]` calls the handle directly, so it is a plain E0308 (see
//! `commands_curried_top_level`).

use hicli::bpaf::{pure, Bpaf};
use hicli::{command, App, Arg};

struct Ctx;

/// A configured music source.
#[derive(Debug, Clone, Bpaf, Arg)]
struct SourceName(#[bpaf(positional("SOURCE-NAME"))] String);

/// sync a source
#[command(curried)]
fn sync(_s: SourceName, _ctx: &Ctx) {}

fn main() {
    let _: App<(), Ctx> = App::new(pure(())).command(__hicli_sync());
}
