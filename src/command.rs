//! The command tree: [`Command`], its three builders, and [`IntoCommand`].
//!
//! A [`Command`] is the *only* node type. A leaf command and a group of
//! commands are the same type, which is what lets a parent hold a plain
//! `Vec<Command<Ctx>>` and what will let `commands![…]` be homogeneous.
//!
//! A `Command` does **not** hold a finished `bpaf` parser. A leaf holds a
//! deferred builder, `FnOnce(Mount) -> Box<dyn Parser<Invocation<Ctx>>>`,
//! called when the tree is folded by [`App`](crate::App). That is what bakes
//! each invocation's [`path`](crate::Invocation::path) in at mount time — a
//! `Command` does not know where it will be mounted until its parent says so —
//! and it is the seam that future group-level arguments ("currying", see
//! ADR-004) extend: they add a field to [`Mount`], not a parameter to a public
//! signature.

use crate::external::ExternalCommand;
use crate::invocation::Invocation;
use crate::outcome::IntoOutcome;
use bpaf::{construct, Parser};

/// A boxed parser producing the selected [`Invocation`].
type InvocationParser<Ctx> = Box<dyn Parser<Invocation<Ctx>>>;

/// A leaf's deferred parser builder — see the module docs.
type Deferred<Ctx> = Box<dyn FnOnce(Mount) -> InvocationParser<Ctx>>;

/// Where a command is being mounted: the path from the root, including the
/// command's own name.
///
/// Deliberately crate-private and `#[non_exhaustive]`: it is the extension
/// point for group-level arguments, which must be able to add a field here
/// without changing anything a consumer can see or name.
#[non_exhaustive]
pub(crate) struct Mount {
    /// The path from the root down to (and including) the command being mounted.
    path: Vec<&'static str>,
}

impl Mount {
    /// The root mount: no path yet, one level above the top-level commands.
    pub(crate) fn root() -> Mount {
        Mount { path: Vec::new() }
    }

    /// The mount point for a child named `name` under this one.
    fn child(&self, name: &'static str) -> Mount {
        let mut path = self.path.clone();
        path.push(name);
        Mount { path }
    }
}

/// One node of the command tree: a leaf, a group of commands, or an external
/// subcommand.
///
/// Build one with [`command`], [`group`] or [`external`], optionally add
/// [`details`](Command::details), and hand it to
/// [`App::command`](crate::App::command) / [`App::commands`](crate::App::commands)
/// or to a parent [`group`].
pub struct Command<Ctx> {
    name: &'static str,
    help: String,
    details: Option<String>,
    kind: Kind<Ctx>,
}

/// What a [`Command`] actually is. Private: a group and a leaf are one type to
/// everyone outside this module.
enum Kind<Ctx> {
    /// A runnable command: a deferred builder for its argument parser.
    Leaf(Deferred<Ctx>),
    /// A set of commands mounted under this one's name.
    Group(Vec<Command<Ctx>>),
    /// A git-style external subcommand.
    External(ExternalCommand),
}

/// A runnable command named `name`, parsed by `parser` and run by `run`.
///
/// `help` is required (ADR-001) and used twice on purpose: as the command's
/// `descr` (the heading of `app <cmd> --help`) and as its `help` (the one-line
/// entry in the parent's command list). Longer prose goes in
/// [`details`](Command::details).
///
/// `run` receives the parsed arguments and the shared context, and may return
/// anything implementing [`IntoOutcome`] — `()`, [`Success`](crate::Success),
/// [`Outcome`](crate::Outcome) or `Outcome<Success>`.
///
/// ```
/// # use hicli::{App, Outcome};
/// # use hicli::bpaf::{positional, pure, Parser};
/// struct Ctx { home: String }
///
/// fn greet() -> hicli::Command<Ctx> {
///     let who = positional::<String>("WHO");
///     hicli::command("greet", "greet someone", who, |who: String, ctx: &Ctx| {
///         println!("[{}] hello {who}", ctx.home);
///     })
/// }
///
/// let app = App::new(pure(())).command(greet());
/// let (_globals, invocation) = app.parse(hicli::cli! { greet world }).expect("parses");
/// assert_eq!(invocation.path(), ["greet"]);
/// ```
///
/// `run` must be `Clone` because `bpaf`'s `map` takes an `Fn`: the parser may be
/// asked for a value more than once. A non-capturing closure — what the
/// `#[command]` macro emits — is always `Clone`.
pub fn command<Ctx, A, R>(
    name: &'static str,
    help: impl Into<String>,
    parser: impl Parser<A> + 'static,
    run: impl Fn(A, &Ctx) -> R + Clone + 'static,
) -> Command<Ctx>
where
    A: 'static,
    R: IntoOutcome,
    Ctx: 'static,
{
    Command {
        name,
        help: help.into(),
        details: None,
        kind: Kind::Leaf(Box::new(move |at: Mount| {
            let path = at.path;
            parser
                .map(move |args| {
                    let run = run.clone();
                    Invocation::new(path.clone(), move |ctx: &Ctx| run(args, ctx).into_outcome())
                })
                .boxed()
        })),
    }
}

/// A group of commands, mounted under `name`.
///
/// `help` is required (ADR-001), exactly as for [`command`]. A group *is* a
/// [`Command`], so groups nest by putting one in another's `children`.
///
/// This is what [`#[group]`](macro@crate::group) expands to — the attribute is
/// the documented way to declare one, and this is the way down for a group the
/// attribute cannot express. The two share the name `group` across the macro
/// and value namespaces, so one `use hicli::group;` imports both.
///
/// ```
/// # use hicli::{App, Outcome};
/// # use hicli::bpaf::{pure, Parser};
/// # struct Ctx;
/// # fn create() -> hicli::Command<Ctx> {
/// #     hicli::command("create", "create an event", pure(()), |(), _c: &Ctx| {})
/// # }
/// let app = App::new(pure(())).command(hicli::group("event", "manage events", vec![create()]));
/// let (_globals, invocation) = app.parse(hicli::cli! { event create }).expect("parses");
/// assert_eq!(invocation.path(), ["event", "create"]);
/// ```
///
/// # Panics
///
/// Not here — an empty group panics when the tree is folded (at
/// [`App::into_parser`](crate::App::into_parser) / `run`), because a group with
/// no commands cannot be mounted.
pub fn group<Ctx: 'static>(
    name: &'static str,
    help: impl Into<String>,
    children: Vec<Command<Ctx>>,
) -> Command<Ctx> {
    Command {
        name,
        help: help.into(),
        details: None,
        kind: Kind::Group(children),
    }
}

/// A git-style external subcommand as a [`Command`].
///
/// Name and help come from the [`ExternalCommand`] manifest, so an external is
/// listed in help and completion exactly like any other command — and, as
/// everywhere else in hicli, without the binary ever being executed to find
/// out what it is.
pub fn external<Ctx: 'static>(ext: ExternalCommand) -> Command<Ctx> {
    Command {
        name: ext.name(),
        help: ext.help().to_string(),
        details: None,
        kind: Kind::External(ext),
    }
}

impl<Ctx> Command<Ctx> {
    /// Attach a longer description, shown as the header of
    /// `app <cmd> --help` under the one-line `help`.
    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// The command's name, as it appears on the command line.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl<Ctx: 'static> Command<Ctx> {
    /// Fold this command into a `bpaf` subcommand parser mounted under
    /// `parent`.
    ///
    /// This is where the tree becomes a parser and where each invocation's path
    /// is baked in: a leaf's deferred builder is called with this command's own
    /// mount point, a group mounts its children under it first, and an external
    /// binds the child-process run.
    ///
    /// # Panics
    ///
    /// Panics if this command is a group with no children.
    pub(crate) fn mount(self, parent: &Mount) -> InvocationParser<Ctx> {
        let Command {
            name,
            help,
            details,
            kind,
        } = self;
        let at = parent.child(name);
        let inner: InvocationParser<Ctx> = match kind {
            Kind::Leaf(build) => build(at),
            Kind::Group(children) => alternative(children, &at).unwrap_or_else(|| {
                panic!("hicli::group `{name}` has no commands; add at least one child")
            }),
            Kind::External(ext) => external_parser(ext, at.path),
        };
        let opts = inner.to_options().descr(help.as_str());
        let opts = match &details {
            Some(d) => opts.header(d.as_str()),
            None => opts,
        };
        opts.command(name).help(help.as_str()).boxed()
    }
}

/// Fold `children` into one alternative parser, mounted under `at`.
///
/// The first branch that matches wins — this is what replaces the central
/// `match`: every child folds one more command into the alternative. `None`
/// when there are no children; the caller decides what that means.
pub(crate) fn alternative<Ctx: 'static>(
    children: Vec<Command<Ctx>>,
    at: &Mount,
) -> Option<InvocationParser<Ctx>> {
    let mut acc: Option<InvocationParser<Ctx>> = None;
    for child in children {
        let next = child.mount(at);
        acc = Some(match acc {
            None => next,
            Some(acc) => construct!([acc, next]).boxed(),
        });
    }
    acc
}

/// The parser for an external subcommand: capture the remainder verbatim, and
/// produce an [`Invocation`] that runs the child process when — and only when —
/// it is run.
fn external_parser<Ctx: 'static>(
    ext: ExternalCommand,
    path: Vec<&'static str>,
) -> InvocationParser<Ctx> {
    ext.into_run_parser()
        .map(move |run| Invocation::new(path.clone(), move |_ctx: &Ctx| run.run()))
        .boxed()
}

/// Anything that can be turned into a [`Command`].
///
/// Two impls: a `Command` itself, and any zero-argument callable returning one
/// — which is what a `#[command]`- or `#[group]`-generated handle
/// (`__hicli_create`) is. That is what lets
/// [`App::command`](crate::App::command) take either, and what every element of
/// [`commands!`](crate::commands) that is *not* a bare path goes through.
pub trait IntoCommand<Ctx> {
    /// Turn `self` into the command it denotes.
    fn into_command(self) -> Command<Ctx>;
}

impl<Ctx> IntoCommand<Ctx> for Command<Ctx> {
    fn into_command(self) -> Command<Ctx> {
        self
    }
}

impl<Ctx, F: FnOnce() -> Command<Ctx>> IntoCommand<Ctx> for F {
    fn into_command(self) -> Command<Ctx> {
        self()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{Failure, Success};
    use bpaf::pure;

    /// Mount `c` as the only top-level command and parse `args` through it.
    fn parse<Ctx: 'static>(c: Command<Ctx>, args: &[&str]) -> Invocation<Ctx> {
        c.mount(&Mount::root())
            .to_options()
            .run_inner(args)
            .expect("parses")
    }

    #[test]
    fn a_leaf_parses_and_dispatches() {
        let c = command("greet", "greet someone", pure(42u8), |n: u8, ctx: &u8| {
            Success::code(n + *ctx)
        });
        let invocation = parse(c, &["greet"]);
        assert_eq!(invocation.name(), "greet");
        assert_eq!(invocation.run(&1), Ok(Success::code(43)));
    }

    #[test]
    fn a_nested_path_is_baked_in_at_mount_time() {
        let c = group(
            "event",
            "manage events",
            vec![command(
                "create",
                "create an event",
                pure(()),
                |(), _c: &()| {},
            )],
        );
        assert_eq!(parse(c, &["event", "create"]).path(), ["event", "create"]);
    }

    #[test]
    fn a_top_level_path_is_the_command_itself() {
        let c = command("apply", "apply something", pure(()), |(), _c: &()| {});
        assert_eq!(parse(c, &["apply"]).path(), ["apply"]);
    }

    #[test]
    #[should_panic(expected = "has no commands")]
    fn an_empty_group_cannot_be_mounted() {
        let c: Command<()> = group("event", "manage events", Vec::new());
        let _ = c.mount(&Mount::root());
    }

    #[test]
    fn into_command_accepts_a_command_and_a_callable() {
        fn handle() -> Command<()> {
            command("apply", "apply something", pure(()), |(), _c: &()| {})
        }
        let direct: Command<()> = command("apply", "apply something", pure(()), |(), _c: &()| {});
        assert_eq!(IntoCommand::into_command(direct).name(), "apply");
        assert_eq!(IntoCommand::into_command(handle).name(), "apply");
    }

    #[test]
    fn details_are_optional_and_kept() {
        let c: Command<()> =
            command("apply", "apply something", pure(()), |(), _c: &()| {}).details("at length");
        assert_eq!(c.details.as_deref(), Some("at length"));
    }

    #[test]
    fn a_leaf_may_fail() {
        let c = command("apply", "apply something", pure(()), |(), _c: &()| {
            Err::<Success, _>(Failure::runtime("boom"))
        });
        assert_eq!(parse(c, &["apply"]).run(&()), Err(Failure::runtime("boom")));
    }
}
