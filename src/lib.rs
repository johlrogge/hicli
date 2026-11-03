//! `hicli` — CLI-enable your code. A modular, publishable subcommand framework
//! built on [`bpaf`].
//!
//! One subcommand = one documented **function**, and one group of subcommands =
//! one documented **module**. [`#[command]`](macro@command) emits the annotated
//! function exactly as written — same name, same signature, same visibility —
//! and *adds* a hidden registration handle beside it;
//! [`#[group]`](macro@group) does the same for a module and **discovers** its
//! children, which are every marked item in the body, in source order. A child
//! is a child because it is in the module, so there is no list to keep in step
//! with the code and no central dispatch `match` anywhere. The result is a tree
//! of [`Command`]s composed by value. Parsing one yields an [`Invocation`]: the
//! selected command with its arguments already bound, which a test can inspect
//! and the run path can run.
//!
//! `hicli` adds four things a `bpaf` CLI usually hand-rolls:
//!
//! - **A tree that is a declaration.** [`#[command]`](macro@command) on a
//!   function, [`#[group]`](macro@group) on an inline module, and
//!   [`group_mod!`](macro@group_mod) on a `mod` declaration whose body is
//!   another file. Each writes a hidden `__hicli_<ident>` handle beside the
//!   item, which [`commands!`](macro@commands) and [`app!`] mount by **bare
//!   name** — a group and a command are indistinguishable in a list, which is
//!   the point. Every one of them expands to public API ([`command`](fn@command),
//!   [`group`](fn@group), [`App::new`], [`App::commands`]), so anything that
//!   outgrows a macro can be written out by hand.
//! - **Exit-code remap.** `bpaf` uses `1` for usage errors and `0` for
//!   help/version; [`App::run`] remaps usage to `2` (see [`exit_code_for`]).
//!   A command reports its own outcome as a value: a [`Success`] carrying an
//!   exit code and printing nothing (how an external subcommand forwards its
//!   child's code), or a [`Failure`] — usage `2`, runtime `1` — whose message
//!   goes to stderr. `hicli` never calls `std::process::exit`: [`App::run`]
//!   *returns* a `std::process::ExitCode`, so destructors run and the consumer's
//!   `main` stays in control.
//! - **First-class in-process testing.** [`App::parse`] / [`App::into_parser`]
//!   drive the whole app with no process side effects, and
//!   [`App::run_with_args`] drives the whole run path — parse, remap, `mk_ctx`,
//!   the command, exit code — returning the code instead of touching the
//!   process, so `App::run` itself is covered by ordinary tests. The [`cli!`] /
//!   [`cli_test!`] macros let tests read like the terminal.
//! - **Typo suggestions.** On a usage error with no built-in `bpaf`
//!   suggestion, a Levenshtein pass over the registered **top-level** command
//!   names — applied to the command position only — appends a `did you mean`
//!   hint. This gap-closer runs only on the run path ([`App::run`] /
//!   [`App::run_with_args`]); a usage error returned by [`App::parse`] carries
//!   `bpaf`'s own suggestions, never hicli's hint.
//!
//! `hicli` re-exports [`bpaf`] so consumers pin a single version through it.
//!
//! # One word, four things
//!
//! Four items share the root word `command`, in three namespaces, and they do
//! not collide:
//!
//! | Spelling | Namespace | What it is |
//! |---|---|---|
//! | [`Command<Ctx>`](Command) | type | one node of the command tree |
//! | [`hicli::command`](fn@command) | value | the function that builds a leaf |
//! | [`#[command]`](macro@command) | macro | the attribute on a function |
//! | [`commands!`](macro@commands) | macro | the registration list |
//!
//! This is the rule [`#[group]`](macro@group) and [`group`](fn@group) already
//! rely on, and it is an ergonomic win, not an accident: **one
//! `use hicli::command;` imports both the attribute and the function**, so a
//! file that declares commands with the attribute and hand-builds one with the
//! function needs no second import and no renaming.
//!
//! # Example
//! ```
//! use hicli::bpaf::{positional, pure, Parser};
//! use hicli::{app, cli, command, group, App, Arg, Outcome};
//!
//! /// The shared context — built once, handed to every command.
//! struct Ctx {
//!     home: String,
//! }
//!
//! /// An argument type. A consumer normally writes `#[derive(Bpaf, Arg)]`
//! /// here; this example spells the impl out so it needs no extra dependency —
//! /// and shows what the derive writes.
//! struct Title(String);
//!
//! impl Arg for Title {
//!     fn parser() -> impl Parser<Self> {
//!         positional::<String>("TITLE").map(Title)
//!     }
//! }
//!
//! /// events and instances
//! #[group] // a group is an annotated module …
//! mod event {
//!     use super::*;
//!
//!     /// create an event
//!     #[command] // … and a command is an annotated function, left as written
//!     pub fn create(t: Title, ctx: &Ctx) -> Outcome {
//!         println!("[{}] created {}", ctx.home, t.0);
//!         Ok(())
//!     }
//! }
//!
//! fn main() {
//!     // `event` is a name, not a call: `commands!` reads it as the hidden
//!     // handle `#[group]` wrote beside the module.
//!     let a: App<(), Ctx> = app! { globals: pure(()), commands: [event] };
//!
//!     let (_globals, invocation) = a.parse(cli! { event create "My Event" }).unwrap();
//!     assert_eq!(invocation.name(), "create");
//!     // `path()` says *which* `create` — `event create`, not some other group's.
//!     assert_eq!(invocation.path(), ["event", "create"]);
//!
//!     // … and `create` is still the function you wrote, callable by its own
//!     // name, with its own signature, by its tests and its neighbours.
//!     event::create(Title("My Event".into()), &Ctx { home: ".".into() }).expect("ok");
//! }
//! ```
//!
//! A group whose body is a **file** is declared with
//! [`group_mod!`](macro@group_mod) instead — the same group, one file per group:
//!
//! ```ignore
//! group_mod! {
//!     /// events and instances
//!     pub mod event;
//! }
//! ```
//!
//! That is the Rust 1.97 spelling. From Rust 1.99 the attribute may be written
//! on the declaration itself, and the body of `group_mod!` is already exactly
//! the item it goes on — migration is deleting the wrapper.

pub use bpaf;

/// Register one documented function as a [`Command`] — and leave the function
/// exactly as it is.
///
/// ```
/// use hicli::bpaf::{positional, pure, Parser};
/// use hicli::{cli, command, App, Arg, Outcome};
///
/// /// The shared context — built once, handed to every command.
/// struct Ctx { home: String }
///
/// /// An argument type. A consumer normally writes `#[derive(Bpaf, Arg)]`
/// /// here; this example spells the impl out so it needs no extra dependency.
/// struct Title(String);
///
/// impl Arg for Title {
///     fn parser() -> impl Parser<Self> { positional::<String>("TITLE").map(Title) }
/// }
///
/// /// create an event
/// ///
/// /// The event directory is created if it does not exist.
/// #[command]
/// pub fn create(t: Title, ctx: &Ctx) -> Outcome {
///     println!("[{}] created {}", ctx.home, t.0);
///     Ok(())
/// }
///
/// fn main() {
///     // The command is registered by name — no parentheses, no repeated name,
///     // no repeated help.
///     let app: App<(), Ctx> = hicli::app! {
///         globals: pure(()),
///         commands: [create],
///     };
///     let (_globals, invocation) = app.parse(cli! { create "Summer Party" }).expect("parses");
///     assert_eq!(invocation.path(), ["create"]);
///
///     // … and `create` is still the function you wrote: same name, same
///     // arguments, callable by its neighbours and by its tests.
///     create(Title("Summer Party".into()), &Ctx { home: ".".into() }).expect("ok");
/// }
/// ```
///
/// # What it reads
///
/// | From | Becomes |
/// |---|---|
/// | the ident, `_` → `-` (`add_admin` → `add-admin`) | the command's name; override it with `#[command(name = "…")]` |
/// | the doc comment's first line | the one-line `help` **and** the heading of `app <cmd> --help` |
/// | the rest of the doc comment | [`Command::details`] |
/// | each **owned** parameter `p: T` | one argument type; its parser is `<T as Arg>::parser()`, composed in declaration order |
/// | the single `&T` parameter | the context; `T` is the command's `Ctx`. Conventionally last, but first works too |
/// | the return type | anything implementing [`IntoOutcome`]: `()`, [`Success`], [`Outcome`] or `Outcome<Success>` |
///
/// A command with **no** `&T` parameter is generic over `Ctx`, so it can be
/// mounted in any app.
///
/// The parameters are composed with `bpaf`'s `construct!` in declaration
/// order, so `bpaf`'s rule applies: every argument struct containing a
/// positional must come **after** the ones that do not. Declare switches and
/// options first, positionals last. (It is a rule about the parameter order,
/// not the typed one — a `--flag` is still accepted anywhere on the command
/// line.)
///
/// `#[command]` has no parameter attributes and never sees an argument's
/// spelling: positionals, switches, help, guards and defaults all live in the
/// argument type, written with `#[derive(Bpaf)]` (which needs a direct
/// `bpaf = "0.9"` dependency, because the derive expands to `::bpaf::…` paths).
///
/// # A parameter's type is resolved through [`Arg`](trait@Arg)
///
/// Each owned parameter's parser is `<T as Arg>::parser()` — one trait call,
/// not a name guessed at the call site. Everything about *how* an argument
/// parses is therefore one line on the type, written once and inherited by
/// every command that takes it, including the ones not written yet. The usual
/// spelling is the derive:
///
/// ```ignore
/// #[derive(Debug, Clone, Bpaf, Arg)]
/// pub struct Title(#[bpaf(positional("TITLE"))] pub String);
/// ```
///
/// A **primitive** parameter is a compile error, deliberately: `hicli` ships
/// no `Arg` impls for `String`, `u64`, `PathBuf` or anything else, so
/// `fn create(name: String, ctx: &Ctx)` fails with
/// *"`String` is not a hicli argument type"* pointing at `name: String` and
/// naming the newtype to write. The metavar and the positional-vs-option
/// question are yours, and a blanket impl would have to invent both.
///
/// The snake_case rule that used to live here — `EventRef` → `event_ref()` —
/// is now [`#[derive(Arg)]`](derive@Arg)'s, which is the one place that calls
/// the constructor `#[derive(Bpaf)]` writes. A type whose parser outgrows that
/// writes [`Arg::parser`](trait@Arg::parser) by hand, and `#[command]` cannot
/// tell the difference.
///
/// # What it writes
///
/// **Your function, untouched** — same name, same signature, same visibility,
/// same attributes in the same order, same body — and one hidden item beside
/// it, the *handle*:
///
/// ```ignore
/// // exactly what you wrote:
/// pub fn create(r: EventRef, t: Title, ctx: &Ctx) -> Outcome { /* your body */ }
///
/// // … and the handle, added:
/// #[doc = "`hicli` registration handle for [`create`]."]
/// #[doc(hidden)]
/// #[allow(unreachable_pub)]
/// pub fn __hicli_create() -> ::hicli::Command<Ctx> {
///     ::hicli::command(
///         "create",
///         "create an event",
///         {
///             let a0 = <EventRef as ::hicli::Arg>::parser();
///             let a1 = <Title as ::hicli::Arg>::parser();
///             ::hicli::bpaf::construct!(a0, a1,)
///         },
///         |(a0, a1), ctx: &Ctx| create(a0, a1, ctx),
///     )
///     .details("The event directory is created if it does not exist.")
/// }
/// ```
///
/// The parsers are bound to locals rather than inlined because `bpaf`'s
/// `construct!` accepts only a single-segment `ident()` call as an element. At
/// arity 0 the parser is `::hicli::bpaf::pure(())`; at every other arity the
/// trailing comma is load-bearing, since `construct!(a0)` yields a parser *of a
/// parser*.
///
/// Both are plain Rust calling this crate's public API, so a command that
/// outgrows the macro can be written out by hand with [`command`](fn@command)
/// and nothing is lost.
///
/// A `#[cfg(…)]` or `#[cfg_attr(…)]` on the function is *copied* onto the
/// handle, so a command that is configured out takes its registration with it.
///
/// # Registration is by handle
///
/// The handle is **always** `pub` — a private command in a private module is
/// still registered from the module that contains it — and its name is a
/// documented convention: `__hicli_` followed by the function's ident,
/// unescaped (`fn r#move` → `__hicli_move`). [`commands!`](macro@commands) is
/// what applies it: a bare path in a list has its last segment rewritten, so
/// `commands![create]` is `__hicli_create()` and
/// `commands![other::cli::event]` is `other::cli::__hicli_event()`.
/// [`app!`] routes its list through the same macro; an enclosing
/// [`#[group]`](macro@group) calls the same handle directly.
///
/// A handle is an ordinary function returning a [`Command`], so
/// `App::command(__hicli_create)` works where one is genuinely wanted — but
/// the bare-name form is the one to write.
///
/// # Naming
///
/// The function keeps its own name, so a command that calls itself recursively
/// calls *itself*, and a command can call its neighbour by name — `create(dir,
/// name, ctx)` is just a function call.
///
/// What still shares the value namespace is the constructor
/// `#[derive(Bpaf)]` writes for an argument type: a struct `Create` gets
/// `fn create()`, which collides with `#[command] fn create` with an ordinary
/// *"name defined multiple times"* error. `#[command]` no longer names that
/// constructor — [`#[derive(Arg)]`](derive@Arg) does — but the clash is between
/// two items in one scope, so it is unchanged. Name an argument type after what
/// it *carries*, not after the command that takes it — `EventDir`, `EventName`,
/// or `CreateArgs` when it is genuinely the bag of arguments for `create`.
/// `tests/cli.rs` uses `CreateArgs` for this reason.
///
/// The doc comment on an argument type — and on a `Globals` struct — is
/// rendered by `bpaf` as a header in `--help`, so keep it to a single
/// user-facing line and leave developer prose to `//` comments beside it.
///
/// # Rejections
///
/// Each of these is a compile error pointing at the token to change:
///
/// - **no doc comment** — a command's help is its documentation, so there is
///   none to fall back on;
/// - **more than one `&T` parameter**, or a `&mut T` — the context is single
///   and shared;
/// - **a `self` receiver**, generics or lifetimes, `async`, `unsafe`, an
///   `extern` ABI;
/// - **an owned parameter that is not a named type** (a tuple, `impl Trait`) —
///   there is no type to ask for an [`Arg`](trait@Arg) impl;
/// - **an owned parameter whose type is not an [`Arg`](trait@Arg)** — a
///   primitive such as `String`, reported by the trait itself with the newtype
///   to write;
/// - **a pattern parameter** (`Words(w): Words`) — the handle's closure calls
///   the function by naming its arguments; take `w: Words` and use `w.0`.
pub use hicli_macros::command;

/// Register a documented module as a group of commands — and leave the module
/// exactly as it is.
///
/// ```
/// use hicli::bpaf::pure;
/// use hicli::{app, cli, command, group, App, Outcome};
///
/// /// The shared context — built once, handed to every command.
/// struct Ctx;
///
/// /// events and instances
/// ///
/// /// An event is a directory; an instance is a branch of it.
/// #[group]
/// pub mod event {
///     use super::*;
///
///     /// create an event
///     #[command]
///     pub fn create(ctx: &Ctx) -> Outcome {
///         let _ = ctx;
///         Ok(())
///     }
///
///     /// manage audiences
///     #[group]
///     pub mod audience {
///         use super::*;
///
///         /// define an audience
///         #[command]
///         pub fn define(ctx: &Ctx) -> Outcome {
///             let _ = ctx;
///             Ok(())
///         }
///     }
/// }
///
/// fn main() {
///     // `event` is registered by name, like a command: a group and a command
///     // are the same thing in a list.
///     let a: App<(), Ctx> = app! { globals: pure(()), commands: [event] };
///     let (_globals, invocation) = a.parse(cli! { event audience define }).expect("parses");
///     assert_eq!(invocation.path(), ["event", "audience", "define"]);
/// }
/// ```
///
/// # What it reads
///
/// | From | Becomes |
/// |---|---|
/// | the ident, `_` → `-` (`add_admin` → `add-admin`) | the group's name. There is no `name = "…"` override |
/// | the doc comment's first line | the one-line `help` **and** the heading of `app <group> --help` |
/// | the rest of the doc comment | [`Command::details`] |
/// | every marked item in the body, **in source order** | the group's children |
/// | the first `&T` among the children | the group's `Ctx` (see below) |
///
/// **A child is a child because it is in the module.** A `fn` carrying
/// `#[command]` and a `mod` carrying `#[group]` are children; everything else —
/// argument structs, `impl` blocks, an ordinary `mod`, `#[cfg(test)] mod tests`
/// — is not. There is no list to keep in step with the code, so a command that
/// was written can never be a command that was never registered, and moving a
/// command in `--help` means moving a function.
///
/// A child's `#[cfg(…)]` / `#[cfg_attr(…)]` rides on its *registration*, so a
/// command that is configured out is not advertised either; the module's own
/// `#[cfg]` does the same for the whole group.
///
/// # The context type
///
/// A group's handle has to name the `Ctx` of the [`Command`] it returns, and a
/// `fn` cannot infer its return type. In order:
///
/// 1. `#[group(ctx = T)]`, when you say so — the only argument the attribute
///    takes;
/// 2. otherwise the **first** `&T` in a depth-first, source-order walk of the
///    children: a group's context is whatever its commands already take;
/// 3. otherwise the handle is **generic over `Ctx`**, exactly as a context-free
///    `#[command]`'s is, so a context-free group mounts in any app.
///
/// A lifted type has to mean the same thing one module up, where the handle is
/// written: a single ident (`Ctx`), a `crate::…` path or a leading-`::` path.
/// A module-relative one (`super::Ctx`) is a compile error naming
/// `#[group(ctx = …)]`. Two children naming two *different* context types is
/// not diagnosed — `Ctx` and `crate::Ctx` are the same type spelled two ways —
/// so the first wins and the second is an ordinary type mismatch at its own
/// registration.
///
/// # What it writes
///
/// **Your module, untouched** — including the `#[command]` and `#[group]`
/// attributes inside it, which rustc expands next — and one hidden item beside
/// it in the *parent* scope, the *handle*:
///
/// ```ignore
/// #[doc = "`hicli` registration handle for [`event`]."]
/// #[doc(hidden)]
/// #[allow(unreachable_pub)]
/// pub fn __hicli_event() -> ::hicli::Command<Ctx> {
///     #[allow(unused_mut)]
///     let mut __children: ::std::vec::Vec<::hicli::Command<Ctx>> = ::std::vec::Vec::new();
///     __children.push(event::__hicli_create());
///     __children.push(event::__hicli_audience());
///     ::hicli::group("event", "events and instances", __children)
///         .details("An event is a directory; an instance is a branch of it.")
/// }
/// ```
///
/// It is plain Rust calling this crate's public API, so a group that outgrows
/// the macro can be written out by hand with [`group`](fn@group) and nothing is
/// lost. The handle reaches exactly **one level down**, which is why every
/// handle can be `pub` without leaking anything: `pub` inside a private module
/// is still private.
///
/// # One name, two things
///
/// `#[group]` the attribute and [`group`](fn@group) the function share the name
/// `group` in two different namespaces — the macro namespace and the value
/// namespace — and do not collide. **One `use hicli::group;` imports both**,
/// so a file that declares groups with the attribute and hand-builds one with
/// the function needs no second import and no renaming. The same is true of
/// [`#[command]`](macro@command) and [`command`](fn@command).
///
/// # Registering something the macro cannot express
///
/// An external subcommand, or any hand-built [`Command`], cannot be a
/// discovered child — there is no item to mark. Write the group's handle
/// yourself, by the documented convention, and `commands![tools]` finds it:
///
/// ```
/// # use hicli::bpaf::pure;
/// # use hicli::{cli, commands, App, Command, ExternalCommand};
/// # struct Ctx;
/// /// print the version
/// #[hicli::command]
/// fn version() {}
///
/// /// `hicli` registration handle for the hand-built `tools` group.
/// #[doc(hidden)]
/// pub fn __hicli_tools() -> Command<Ctx> {
///     hicli::group(
///         "tools",
///         "the odds and ends",
///         commands![
///             version,
///             hicli::external(ExternalCommand::new("fmt", "format things", "/opt/bin/fmt")),
///         ],
///     )
/// }
///
/// fn main() {
///     let a: App<(), Ctx> = hicli::app! { globals: pure(()), commands: [tools] };
///     let (_globals, invocation) = a.parse(cli! { tools fmt }).expect("parses");
///     assert_eq!(invocation.path(), ["tools", "fmt"]);
/// }
/// ```
///
/// # Rejections
///
/// Each of these is a compile error pointing at the token to change:
///
/// - **no doc comment, or a blank one** — a group's help is its documentation
///   (ADR-001). This was a runtime panic before the attribute could read the
///   text;
/// - **an empty group** — a group with no commands cannot be mounted, so it is
///   refused where it is written. ([`group`](fn@group) still panics at fold
///   time for a hand-built one);
/// - **`#[group]` on anything but a module**;
/// - **any argument but `ctx = <Type>`**;
/// - **a module-relative context type** among the children;
/// - **`#[group]` on a file module** (`#[group] pub mod event;`) — an
///   attribute on a `mod` declaration needs Rust 1.99. Until then the file form
///   is written [`group_mod!`](macro@group_mod), which is the same group with a
///   wrapper around the declaration.
pub use hicli_macros::group;

/// Declare **file** modules as groups: the same group, whose body is a file.
///
/// ```ignore
/// use hicli::group_mod;
///
/// group_mod! {
///     /// events and instances
///     ///
///     /// An event is a directory; an instance is a branch of it.
///     pub mod event;
///
///     /// identity
///     #[group(ctx = Ctx)]
///     pub mod id;
/// }
/// ```
///
/// Everything [`#[group]`](macro@group) reads, it reads the same way here —
/// the name from the ident, the help from the doc comment, the children from
/// the module's items in source order, the context from the first `&T` among
/// them — with one difference: the items are in another file, which the macro
/// **reads** and parses. `examples/files/` in the repository is a working CLI
/// written this way. (The example above is `ignore`d because a doctest's source
/// is a synthetic file with no module tree beside it.)
///
/// # Grammar
///
/// One or more **outlined** module declarations, each with a doc comment:
///
/// ```text
/// group_mod! {
///     <doc comment>
///     [#[group(ctx = T)]] [#[cfg(…)]] [#[path = "…"]]
///     <visibility> mod <ident> ;
///     …
/// }
/// ```
///
/// The body is **exactly** the item `#[group]` will be written on from Rust
/// 1.99, which is what makes the migration *delete the wrapper, keep the item*
/// — including the context override, which is written as `#[group(ctx = T)]`
/// **on the declaration** rather than as an argument to `group_mod!`. That
/// attribute is read and then removed; everything else on the declaration is
/// re-emitted untouched.
///
/// There is no argument to the invocation itself, and therefore no place to put
/// a `#[cfg]` for all of it: **a `#[cfg]` goes on each declaration**, where it
/// rides onto that group's own registration exactly as `#[group]`'s does.
///
/// # Finding the file
///
/// The Reference's rules, applied to the file the macro was invoked in:
///
/// - an explicit `#[path = "…"]` **wins**, and is relative to the declaring
///   file's directory;
/// - a **mod-rs** file (`mod.rs`, `lib.rs`, `main.rs`, or a crate root such as
///   `examples/demo.rs`) searches its own directory — `src/lib.rs` finds
///   `src/event.rs` or `src/event/mod.rs`;
/// - a **non-mod-rs** file `x.rs` searches `x/` — `src/event.rs` finds
///   `src/event/audience.rs` or `src/event/audience/mod.rs`.
///
/// The macro then **emits `#[path = "<relative>"]`** for every declaration it
/// resolved itself, so rustc reads exactly the bytes the macro read. That also
/// settles the one question a proc macro cannot ask the compiler — *is this
/// declaring file a crate root?* — because whichever file the macro chose is
/// the file rustc is told to use. The emitted path is always relative to the
/// declaring file's directory; an absolute path never appears in the output.
///
/// # Rebuilds
///
/// Nothing to do: the file is `mod`-linked into the crate, so it is in cargo's
/// dep-info and editing it re-runs the macro like any other source change.
///
/// # It must be at a file's top level
///
/// Rust resolves `mod x;` inside an inline `mod a { … }` against `a/`, and a
/// proc macro cannot see the inline module path it sits in. Nested use fails
/// with *"cannot find the file for module"*, whose note says so. Inside an
/// inline module, write [`#[group]`](macro@group) on an inline module instead.
///
/// # Rejections
///
/// [`#[group]`](macro@group)'s rejections all apply — no doc comment, an empty
/// group, a module-relative context type, an argument that is not
/// `ctx = <Type>` — plus:
///
/// - **the file was not found** — the message lists every candidate it tried,
///   and notes the top-level-only rule above;
/// - **the file is not valid Rust**, or could not be read — the message names
///   the file and quotes the compiler's or the operating system's reason;
/// - **something in the body is not a `mod` declaration**;
/// - **a declaration has a body** — that is the inline form, so write
///   `#[group]` on it and drop the wrapper;
/// - **the source file cannot be located** (`--remap-path-prefix`, or a front
///   end that discloses no path) — there is nothing to read, so the message
///   points at the inline form.
///
/// # After Rust 1.99
///
/// `#[group] pub mod event;` becomes an alias for all of this and `group_mod!`
/// is deprecated. Migration is deleting the wrapper and keeping the item:
/// the declaration inside `group_mod!` — doc comment, `#[group(ctx = …)]`,
/// `#[cfg]`, `#[path]` and all — is already exactly what the attribute goes on.
pub use hicli_macros::group_mod;

/// Build a `Vec<Command<Ctx>>` from a list of registrations.
///
/// **A bare path always names a handle.** `create` is the handle `#[command] fn
/// create` wrote (`__hicli_create()`), `audience` is the one
/// [`#[group]`](macro@group) wrote beside `mod audience`, and
/// `audience::define` is either of them one module along — the last
/// segment is rewritten, the rest of the path is kept as written. Anything that
/// is *not* a bare path is an expression, and goes through
/// [`IntoCommand::into_command`]; a trailing comma is accepted, and an empty
/// list yields an empty `Vec` (which a group still refuses at fold time).
///
/// ```
/// # use hicli::{commands, Command, ExternalCommand};
/// # struct Ctx;
/// /// apply everything
/// #[hicli::command]
/// fn apply(ctx: &Ctx) {
///     let _ = ctx;
/// }
///
/// fn main() {
///     let cs: Vec<Command<Ctx>> = commands![
///         apply,
///         hicli::external(ExternalCommand::new("foo", "an external", "/opt/bin/foo")),
///     ];
///     assert_eq!(cs.iter().map(Command::name).collect::<Vec<_>>(), ["apply", "foo"]);
/// }
/// ```
///
/// # A local is not a name
///
/// Because a bare path is a name, a local variable that already holds a
/// [`Command`] cannot be written bare — `commands![c]` looks for a handle
/// called `__hicli_c`. Write it as an expression — `(c)` — or skip the macro
/// and hand the `Vec` over yourself, `App::commands(vec![c])`:
///
/// ```
/// # use hicli::{commands, App, Command};
/// # use hicli::bpaf::pure;
/// # struct Ctx;
/// let c: Command<Ctx> = hicli::command("apply", "apply everything", pure(()), |(), _: &Ctx| {});
/// let cs: Vec<Command<Ctx>> = commands![(c)];          // parenthesised: an expression
/// let _app: App<(), Ctx> = App::new(pure(())).commands(cs);
/// ```
///
/// # Forwarding
///
/// A macro of your own that wraps `commands!` should forward its list as
/// **token trees** (`$($c:tt)*`), not as `$c:expr`: an `:expr` capture wraps
/// each element in an invisible group, which says "expression" about something
/// whose whole meaning here is that it is a *name*. `commands!` peels those
/// groups, so the other form works too — but forwarding token trees is what
/// keeps the list literally the list the user wrote. [`app!`] does it that way.
pub use hicli_macros::commands;

mod app;
mod command;
mod external;
mod invocation;
mod macros;

pub mod arg;
pub mod outcome;
pub mod testing;

pub use app::{exit_code_for, App};
pub use command::{command, external, group, Command, IntoCommand};
pub use external::{ArgSchema, ArgSpec, ExternalCommand};
pub use invocation::Invocation;
pub use outcome::{Failure, IntoOutcome, Outcome, Success};

// The trait and the derive that implements it, kept together on purpose: they
// share the name `Arg` across the type and macro namespaces, so one
// `use hicli::Arg;` imports both. The blank lines are a fence — rustfmt
// reorders within a contiguous block, and would otherwise sort them apart.
pub use arg::{complete_dirs, complete_files, complete_with, Arg, Candidate};

/// Implement [`Arg`](trait@Arg) for an argument type — and leave the type
/// exactly as it is.
///
/// ```
/// use hicli::bpaf::{Bpaf, Parser};
/// // One import, two namespaces: the trait and this derive share the name
/// // `Arg` the way `group` the function and `#[group]` the attribute do, so a
/// // single `use hicli::Arg;` brings both into scope.
/// use hicli::Arg;
///
/// /// An event id.
/// #[derive(Debug, Clone, Bpaf, Arg)]
/// pub struct Id(#[bpaf(positional("ID"))] String);
///
/// let id = <Id as Arg>::parser()
///     .to_options()
///     .run_inner(&["e-17"][..])
///     .expect("parses");
/// assert_eq!(id.0, "e-17");
/// ```
///
/// # What it reads
///
/// | From | Becomes |
/// |---|---|
/// | the type's ident, snake_cased (`EventRef` → `event_ref`, `HTTPServer` → `http_server`) | the constructor the impl calls — the one `#[derive(Bpaf)]` wrote |
/// | `#[arg(complete = <path>)]` | a dynamic completer, through [`complete_with`] |
/// | `#[arg(complete = files)]` / `#[arg(complete = dirs)]` | the shell's own completion, through [`complete_files`] / [`complete_dirs`] |
/// | `#[arg]`, or no attribute at all | nothing — the constructor, unwrapped |
///
/// The bare words `files` and `dirs` are read as the shorthands **before** they
/// are read as paths, so a function of your own that is genuinely called `files`
/// has to be named as one: `self::files`.
///
/// The derive says nothing about spelling — the metavar, positional-vs-option,
/// guards and fallbacks all stay in `#[derive(Bpaf)]`, which needs a direct
/// `bpaf = "0.9"` dependency because its expansion names `::bpaf::…` paths.
///
/// # What it writes
///
/// **Your type, untouched**, and one impl beside it:
///
/// ```ignore
/// #[automatically_derived]
/// impl ::hicli::Arg for Id {
///     fn parser() -> impl ::hicli::bpaf::Parser<Self> {
///         ::hicli::complete_with(id(), complete_ids)
///     }
/// }
/// ```
///
/// It is plain Rust calling this crate's public API — the helper as a
/// **function**, never as a method — so a type whose parser outgrows the derive
/// writes the same impl by hand and loses nothing.
///
/// # The completer filters
///
/// `bpaf` passes a completer's output through unfiltered: it filters its own
/// flag names, not your candidates. `complete_ids` above is handed the value
/// that already parsed and must drop everything that does not match it. The
/// [`arg`](crate::arg) module docs spell out when it is called at all — for a
/// `String` on every keystroke, for a validated type never on the empty word.
///
/// # A missing `#[derive(Bpaf)]` looks like this
///
/// The impl calls `id()`, and nothing else writes that function, so a type that
/// derives `Arg` without `Bpaf` fails with rustc's ordinary
///
/// ```text
/// error[E0425]: cannot find function `id` in this scope
/// ```
///
/// pointing at the type's own ident — because that is where the call is spanned.
/// Deliberate: the answer is to add `#[derive(Bpaf)]` (or write `fn id()`
/// yourself), and rustc says which name is missing better than any wrapper
/// message would.
///
/// # Rejections
///
/// Each of these is a compile error pointing at the token to change:
///
/// - **any `#[arg(…)]` key but `complete`** — there is only one thing to say
///   here;
/// - **a second `#[arg(…)]` attribute** — one attribute, one answer; two would
///   leave *which completer is this?* unanswerable from the source;
/// - **`complete` on a struct with more than one field, or on an enum** — a
///   completer on the whole value is handed the *built* value, so it cannot tell
///   which field is being completed and never fires while another field is
///   mid-word. Make the value a newtype and complete that, or complete the
///   field itself with `bpaf`'s own `#[bpaf(complete(f))]`;
/// - **a union, or a generic type** — an argument type is concrete.
pub use hicli_macros::Arg;
