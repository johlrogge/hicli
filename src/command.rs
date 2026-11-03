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
    footer: Option<String>,
    aliases: Vec<&'static str>,
    listing: Listing,
    kind: Kind<Ctx>,
}

/// Whether a command shows up in help, completion and suggestions. Either way
/// it parses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Listing {
    /// The default: listed everywhere.
    Listed,
    /// Set by [`Command::hide`]: typed exactly, it works; nothing advertises it.
    Hidden,
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
    Command::new(
        name,
        help.into(),
        Kind::Leaf(Box::new(move |at: Mount| {
            let path = at.path;
            parser
                .map(move |args| {
                    let run = run.clone();
                    Invocation::new(path.clone(), move |ctx: &Ctx| run(args, ctx).into_outcome())
                })
                .boxed()
        })),
    )
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
    Command::new(name, help.into(), Kind::Group(children))
}

/// A git-style external subcommand as a [`Command`].
///
/// Name and help come from the [`ExternalCommand`] manifest, so an external is
/// listed in help and completion exactly like any other command — and, as
/// everywhere else in hicli, without the binary ever being executed to find
/// out what it is.
pub fn external<Ctx: 'static>(ext: ExternalCommand) -> Command<Ctx> {
    Command::new(ext.name(), ext.help().to_string(), Kind::External(ext))
}

impl<Ctx> Command<Ctx> {
    /// A listed command with no details, footer or aliases.
    fn new(name: &'static str, help: String, kind: Kind<Ctx>) -> Self {
        Command {
            name,
            help,
            details: None,
            footer: None,
            aliases: Vec::new(),
            listing: Listing::Listed,
            kind,
        }
    }

    /// Attach a longer description, shown as the header of
    /// `app <cmd> --help` under the one-line `help`.
    ///
    /// `bpaf` reflows it as prose: lines are joined, and plain `--help` shows
    /// only its first paragraph. For text that must keep its lines, use
    /// [`footer`](Command::footer).
    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Attach a footer, printed at the end of `app <cmd> --help`, after the
    /// options — **verbatim**, unlike [`details`](Command::details): every line
    /// on its own line, indentation and blank lines kept, in plain `--help` as
    /// well as `--help --help`.
    ///
    /// This is the slot a doc comment's `# Examples` section fills under
    /// [`#[command]`](macro@crate::command) and [`#[group]`](macro@crate::group).
    /// Keep its lines under 100 columns: a longer one wraps, and the
    /// continuation loses its indentation.
    ///
    /// ```
    /// # use hicli::App;
    /// # use hicli::bpaf::pure;
    /// let search = hicli::command("search", "search the library", pure(()), |(), _: &()| {})
    ///     .footer("Examples:\n  mdma search --artist Bowie\n  mdma search --added -7");
    /// let help = App::new(pure(()))
    ///     .command(search)
    ///     .parse(["search", "--help"])
    ///     .err()
    ///     .expect("--help short-circuits")
    ///     .unwrap_stdout();
    /// assert!(help.ends_with("Examples:\n  mdma search --artist Bowie\n  mdma search --added -7\n"));
    /// ```
    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    /// Accept `alias` on the command line as another spelling of this command.
    ///
    /// Repeatable. An alias parses exactly like the name — the
    /// [`Invocation::path`](crate::Invocation::path) is still the canonical
    /// name — but is not listed in help or offered by completion. It is an
    /// exact match for the `did you mean` gap-closer, never a suggestion.
    ///
    /// Two siblings sharing a spelling — name or alias — panic when the tree
    /// is folded.
    ///
    /// # Panics
    ///
    /// Here, if `alias` is this command's own name: it would answer to the
    /// same word twice. (`#[command]` and `#[group]` refuse it at compile
    /// time.)
    pub fn alias(mut self, alias: &'static str) -> Self {
        assert!(
            alias != self.name,
            "a command's alias equals its own name: `{alias}`"
        );
        self.aliases.push(alias);
        self
    }

    /// Leave this command out of help, completion and suggestions. Typed
    /// exactly — by its name or an alias — it still parses and runs.
    pub fn hide(mut self) -> Self {
        self.listing = Listing::Hidden;
        self
    }

    /// The command's name, as it appears on the command line.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Every word this command answers to: its name, then its aliases.
    pub(crate) fn spellings(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.name).chain(self.aliases.iter().copied())
    }

    /// Whether help, completion and suggestions may mention this command.
    pub(crate) fn is_listed(&self) -> bool {
        self.listing == Listing::Listed
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
    /// Panics if this command is a group with no children, or a group with two
    /// children sharing a spelling.
    pub(crate) fn mount(self, parent: &Mount) -> InvocationParser<Ctx> {
        let Command {
            name,
            help,
            details,
            footer,
            aliases,
            listing,
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
        let mut opts = inner.to_options().descr(help.as_str());
        if let Some(d) = &details {
            opts = opts.header(d.as_str());
        }
        if let Some(f) = &footer {
            opts = opts.footer(crate::help::verbatim(f));
        }
        // Always `long`, never `short`: an alias is a whole word, and `long`
        // is the spelling `bpaf` matches a one-character command by too.
        let mut cmd = opts.command(name).help(help.as_str());
        for alias in aliases {
            cmd = cmd.long(alias);
        }
        match listing {
            Listing::Listed => cmd.boxed(),
            Listing::Hidden => cmd.hide().boxed(),
        }
    }
}

/// Fold `children` into one alternative parser, mounted under `at`.
///
/// The first branch that matches wins — this is what replaces the central
/// `match`: every child folds one more command into the alternative. `None`
/// when there are no children; the caller decides what that means.
///
/// # Panics
///
/// Panics if two children share a spelling — a name or an alias — because
/// the second could never be reached by it.
pub(crate) fn alternative<Ctx: 'static>(
    children: Vec<Command<Ctx>>,
    at: &Mount,
) -> Option<InvocationParser<Ctx>> {
    let mut seen: Vec<&'static str> = Vec::new();
    for spelling in children.iter().flat_map(Command::spellings) {
        if seen.contains(&spelling) {
            let place = match at.path.as_slice() {
                [] => "at the top level".to_string(),
                path => format!("in hicli::group `{}`", path.join(" ")),
            };
            panic!(
                "two commands answer to `{spelling}` {place}; give each its own name and aliases"
            );
        }
        seen.push(spelling);
    }
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
    fn an_alias_parses_with_the_canonical_path() {
        let c = group(
            "event",
            "manage events",
            vec![command("list", "list events", pure(()), |(), _c: &()| {})
                .alias("ls")
                .alias("l")],
        )
        .alias("e");
        assert_eq!(parse(c, &["e", "ls"]).path(), ["event", "list"]);
        let c = command("list", "list events", pure(()), |(), _c: &()| {}).alias("l");
        assert_eq!(parse(c, &["l"]).path(), ["list"]);
    }

    #[test]
    fn a_hidden_command_parses_and_dispatches() {
        let c = command("secret", "do it quietly", pure(7u8), |n: u8, _c: &()| {
            Success::code(n)
        })
        .hide();
        assert!(!c.is_listed());
        let invocation = parse(c, &["secret"]);
        assert_eq!(invocation.path(), ["secret"]);
        assert_eq!(invocation.run(&()), Ok(Success::code(7)));
    }

    #[test]
    fn spellings_are_the_name_then_the_aliases() {
        let c: Command<()> = command("list", "list", pure(()), |(), _c: &()| {}).alias("ls");
        assert_eq!(c.spellings().collect::<Vec<_>>(), ["list", "ls"]);
        assert!(c.is_listed());
    }

    #[test]
    #[should_panic(expected = "two commands answer to `ls` in hicli::group `event`")]
    fn siblings_sharing_a_spelling_cannot_be_mounted() {
        let c: Command<()> = group(
            "event",
            "manage events",
            vec![
                command("list", "list events", pure(()), |(), _c: &()| {}).alias("ls"),
                command("ls", "list differently", pure(()), |(), _c: &()| {}),
            ],
        );
        let _ = c.mount(&Mount::root());
    }

    #[test]
    #[should_panic(expected = "a command's alias equals its own name: `list`")]
    fn an_alias_equal_to_the_name_panics() {
        let _: Command<()> =
            command("list", "list events", pure(()), |(), _c: &()| {}).alias("list");
    }

    #[test]
    #[should_panic(expected = "two commands answer to `s` at the top level")]
    fn top_level_siblings_sharing_an_alias_cannot_be_folded() {
        let children: Vec<Command<()>> = vec![
            command("status", "show status", pure(()), |(), _c: &()| {}).alias("s"),
            command("sync", "sync up", pure(()), |(), _c: &()| {}).alias("s"),
        ];
        let _ = alternative(children, &Mount::root());
    }

    #[test]
    fn footer_is_optional_and_kept() {
        let c: Command<()> =
            command("apply", "apply something", pure(()), |(), _c: &()| {}).footer("Examples:");
        assert_eq!(c.footer.as_deref(), Some("Examples:"));
    }

    #[test]
    fn a_leaf_may_fail() {
        let c = command("apply", "apply something", pure(()), |(), _c: &()| {
            Err::<Success, _>(Failure::runtime("boom"))
        });
        assert_eq!(parse(c, &["apply"]).run(&()), Err(Failure::runtime("boom")));
    }
}
