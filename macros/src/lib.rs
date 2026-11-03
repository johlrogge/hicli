//! Procedural macros for [`hicli`](https://docs.rs/hicli):
//! [`macro@command`], [`macro@group`], [`macro@commands`] and [`derive@Arg`].
//!
//! Nothing here is meant to be depended on directly: `hicli` re-exports
//! them and pins this crate to an exact version, so the macros and the runtime
//! they expand into always move together. The generated code names
//! `::hicli::…` paths only.
//!
//! Both attributes are deliberately small, and both leave the item they are
//! written on **exactly as written**, adding one hidden handle beside it.
//! `#[command]` reads a function's ident, its doc comment and its parameter
//! *types* — never an argument's spelling. `#[group]` reads a module's ident,
//! its doc comment and which of its items are marked. See ADR-004
//! (`docs/adr/004-function-first-verbs.md`) and ADR-005
//! (`docs/adr/005-verbs-are-the-function.md`) for why those boundaries are
//! where they are.
//!
//! `proc_macro::TokenStream` appears in this file and nowhere else: every other
//! module works in `proc_macro2`, so it can be unit-tested outside a macro
//! expansion.

mod arg;
mod commands;
mod ctx;
mod discover;
mod expand;
mod fs;
mod group;
mod parse;
mod resolve;
mod shared;

use std::path::PathBuf;

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput, Item, ItemFn};

use crate::fs::RealFs;
use crate::group::{GroupArgs, Source};
use crate::parse::{CommandArgs, CommandFn};

/// The file a macro was invoked in, as far as the compiler and the file system
/// between them can say.
///
/// `Span::local_file()` (stable since Rust 1.88) is the real answer. This
/// crate's MSRV is 1.99 nonetheless, because the file form is an attribute on a
/// `mod` declaration, which rustc accepts from 1.99. `Span::file()` is the
/// fallback, and differs only under `--remap-path-prefix`, where it gives the
/// *remapped* name — a path that will not open, which is why both are filtered
/// through the file system.
///
/// Neither is anchored: the path is relative to *something* — rustc's working
/// directory under cargo, something else under rust-analyzer's proc-macro
/// server — so [`crate::fs::locate`] searches for the anchor and `fs` carries
/// the one it found. Keeping the file relative to that anchor is what keeps
/// diagnostics readable (`src/event.rs`) and guarantees no absolute path
/// reaches the expansion.
struct CallSite {
    /// The file system, anchored where the file was found.
    fs: RealFs,
    /// The declaring file, relative to that anchor — `None` when no candidate
    /// could be found at all, which is D6's *"do not guess"*.
    file: Option<PathBuf>,
    /// The candidates, as the compiler spelled them, for D6 to name.
    tried: Vec<String>,
}

impl CallSite {
    /// Ask the compiler where this expansion is, and find that file.
    fn here() -> Self {
        let span = proc_macro::Span::call_site();
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(local) = span.local_file() {
            candidates.push(local);
        }
        let named = PathBuf::from(span.file());
        if !candidates.contains(&named) {
            candidates.push(named);
        }
        let tried = candidates.iter().map(|p| p.display().to_string()).collect();
        let (fs, file) = fs::locate(&candidates);
        CallSite { fs, file, tried }
    }

    /// What the file form is given: the file, or everywhere that was looked.
    fn source(&self) -> Source<'_> {
        match &self.file {
            Some(file) => Source::Found(file),
            None => Source::Unknown(&self.tried),
        }
    }
}

/// Turn a documented function into a `hicli` command.
///
/// ```ignore
/// /// create an event
/// ///
/// /// The event directory is created if it does not exist.
/// #[command]
/// pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome { … }
/// ```
///
/// The full contract — name, help, arguments, context, return type, the
/// function it leaves alone, the handle it adds, and every rejection — is
/// documented on [`hicli::command`](https://docs.rs/hicli), which is where
/// consumers read it.
#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as CommandArgs);
    let item = parse_macro_input!(item as ItemFn);
    match CommandFn::from_item(args, item).map(expand::expand) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Implement `hicli::Arg` for an argument type, through its `bpaf`
/// constructor.
///
/// ```ignore
/// #[derive(Bpaf, Arg)]
/// #[arg(complete = complete_ids)]
/// struct Id(#[bpaf(positional("ID"))] String);
/// ```
///
/// The full contract — the constructor it calls, the completion shorthands, the
/// impl it writes and every rejection — is documented on
/// [`hicli::Arg`](https://docs.rs/hicli), which is where consumers read
/// it.
#[proc_macro_derive(Arg, attributes(arg))]
pub fn arg(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match arg::expand(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Turn a documented module into a `hicli` group of commands.
///
/// ```ignore
/// /// events and instances
/// ///
/// /// An event is a directory; an instance is a branch of it.
/// #[group]
/// pub mod event {
///     /// create an event
///     #[command]
///     pub fn create(dir: EventDir, ctx: &Ctx) -> Outcome { … }
/// }
/// ```
///
/// The same group with its body in a file (`event.rs` beside the declaring
/// file, or `event/mod.rs`), which the macro reads to discover the children:
///
/// ```ignore
/// /// events and instances
/// #[group]
/// pub mod event;
/// ```
///
/// The full contract — name, help, which items become children, how the
/// context type is decided, the module it leaves alone, the handle it adds, and
/// every rejection — is documented on
/// [`hicli::group`](https://docs.rs/hicli), which is where consumers read
/// it.
#[proc_macro_attribute]
pub fn group(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as GroupArgs);
    let item = parse_macro_input!(item as Item);
    let at = CallSite::here();
    match group::expand(&at.fs, at.source(), args, item) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Build a `Vec<Command<Ctx>>` from a list in which a bare path names a handle.
///
/// The full contract is documented on
/// [`hicli::commands`](https://docs.rs/hicli), which is where consumers
/// read it.
#[proc_macro]
pub fn commands(input: TokenStream) -> TokenStream {
    commands::expand(input.into()).into()
}
