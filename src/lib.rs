//! `verbandi` — a modular, publishable subcommand framework built on
//! [`bpaf`].
//!
//! One subcommand = one [`Verb`], composed by value (not hand-matched into a
//! central `match`). `verbandi` adds three things a `bpaf` CLI usually
//! hand-rolls:
//!
//! - **Exit-code remap.** `bpaf` uses `1` for usage errors and `0` for
//!   help/version; [`App::run`] remaps usage to `2` (see [`exit_code_for`]).
//!   A verb reports its own outcome as an [`Exit`] value — usage `2`, runtime
//!   `1`, or [`Exit::Code`] to carry a specific code silently (how an external
//!   subcommand forwards its child's code). `verbandi` never calls
//!   `std::process::exit`: [`App::run`] *returns* the code, so destructors run
//!   and the consumer's `main` stays in control.
//! - **First-class in-process testing.** [`App::parse`] / [`App::into_parser`]
//!   drive the whole app with no process side effects, and
//!   [`App::run_with_args`] drives the whole run path — parse, remap, `mk_ctx`,
//!   verb, exit code — returning the code instead of touching the process, so
//!   `App::run` itself is covered by ordinary tests. The [`cli!`] /
//!   [`verb_test!`] macros let tests read like the terminal.
//! - **Typo suggestions.** On a usage error with no built-in `bpaf`
//!   suggestion, a Levenshtein pass over the registered **top-level** command
//!   names — applied to the command position only — appends a `did you mean`
//!   hint. This gap-closer runs only on the run path ([`App::run`] /
//!   [`App::run_with_args`]); a usage error returned by [`App::parse`] carries
//!   `bpaf`'s own suggestions, never verbandi's hint.
//!
//! `verbandi` re-exports [`bpaf`] so consumers pin a single version through it.
//!
//! # Example
//! ```
//! use verbandi::{App, Group, Verb, BoxedVerb, Exit};
//! use verbandi::bpaf::{Parser, positional, pure};
//!
//! struct Ctx;
//! struct Create { name: Vec<String> }
//! impl Verb<Ctx> for Create {
//!     fn name(&self) -> &'static str { "create" }
//!     fn run(self: Box<Self>, _ctx: &Ctx) -> Result<(), Exit> {
//!         println!("created {}", self.name.join(" "));
//!         Ok(())
//!     }
//! }
//! fn create() -> impl Parser<BoxedVerb<Ctx>> {
//!     let name = positional::<String>("NAME").some("a name is required");
//!     verbandi::bpaf::construct!(Create { name })
//!         .map(|c| Box::new(c) as BoxedVerb<Ctx>)
//! }
//! fn app() -> App<(), Ctx> {
//!     App::new(pure(())).group(
//!         "event",
//!         "manage events",
//!         Group::new().leaf("create", "create an event", create()),
//!     )
//! }
//!
//! let (_globals, verb) = app().parse(verbandi::cli! { event create "My Event" }).unwrap();
//! assert_eq!(verb.name(), "create");
//! ```

pub use bpaf;

mod app;
mod external;
mod macros;
mod verb;

pub mod testing;

pub use app::{exit_code_for, App, Group};
pub use external::{ArgSchema, ArgSpec, ExternalVerb};
pub use verb::{BoxedVerb, Exit, Verb};
