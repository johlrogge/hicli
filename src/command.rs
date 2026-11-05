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
//! `Command` does not know where it will be mounted until its parent says so.
//!
//! The same seam carries curried commands (ADR-011): a [`group_args`] group
//! parses one value after its name and mounts its [`Curried`] commands at a
//! [`Mount`] that also records the value's metavar, so their usage lines name
//! it (`source SOURCE-NAME sync`). Each yields a [`Partial`] that the group
//! hands the parsed value to. A group's [default](Node::by_default) command is
//! mounted by its group, at the group's own path.

use crate::arg::Candidate;
use crate::external::ExternalCommand;
use crate::invocation::Invocation;
use crate::outcome::IntoOutcome;
use bpaf::doc::{Doc, MetaInfo};
use bpaf::{any, construct, Parser};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A boxed parser producing an `O`.
type Boxed<O> = Box<dyn Parser<O>>;

/// A set of children folded into one alternative, and what its group's help
/// page needs to know about the default among them.
pub(crate) struct Folded<O> {
    /// The alternative itself.
    pub(crate) parser: Boxed<O>,
    /// The default child's help, if there is a default.
    pub(crate) default: Option<DefaultHelp>,
}

/// A default command's one-line help, and whether `bpaf` will print it.
///
/// `bpaf` prints a default's help line only as the title of its options'
/// section, and keeps a section only when it starts with an item (#19). A
/// default without options would be invisible; its group puts the line in
/// its header instead.
pub(crate) struct DefaultHelp {
    /// The default's one-line help.
    help: String,
    /// Set when the default's parser has items to list — read only after
    /// `bpaf` has asked the parser for its metadata, which is what sets it.
    has_items: Rc<Cell<bool>>,
}

/// A deferred parser builder — see the module docs.
type Deferred<O> = Box<dyn FnOnce(&Mount) -> Boxed<O>>;

/// Where a command is being mounted: the path from the root, including the
/// command's own name, and the path as a usage line shows it.
///
/// Deliberately crate-private and `#[non_exhaustive]`: curried groups
/// (ADR-011) added `shown` here — the path as a usage line spells it, with each
/// curried value's metavar — without changing anything a consumer can see or
/// name.
#[non_exhaustive]
pub(crate) struct Mount {
    /// The path from the root down to (and including) the command being
    /// mounted — command names only, what [`Invocation::path`] reports.
    path: Vec<&'static str>,
    /// The same path as a usage line spells it: the binary's name, if there
    /// is one to show, then the path with the metavar of every curried value
    /// on the way down (`playr source SOURCE-NAME sync`, ADR-013).
    shown: Vec<String>,
}

impl Mount {
    /// The root mount: no path yet, one level above the top-level commands.
    /// `bin` is the binary's name, shown first in every usage line — display
    /// only, never part of [`Invocation::path`].
    pub(crate) fn root(bin: Option<&str>) -> Mount {
        Mount {
            path: Vec::new(),
            shown: bin.into_iter().map(str::to_string).collect(),
        }
    }

    /// The mount point for a child named `name` under this one.
    fn child(&self, name: &'static str) -> Mount {
        let mut path = self.path.clone();
        path.push(name);
        let mut shown = self.shown.clone();
        shown.push(name.to_string());
        Mount { path, shown }
    }

    /// The mount point of a curried group's bound commands: the same path,
    /// shown with the group's value (`metavar`) after it.
    fn bound(&self, metavar: &str) -> Mount {
        let mut shown = self.shown.clone();
        shown.push(metavar.to_string());
        Mount {
            path: self.path.clone(),
            shown,
        }
    }

    /// Where this is, for a panic message.
    fn place(&self) -> String {
        match self.path.as_slice() {
            [] => "at the top level".to_string(),
            path => format!("in hicli::group `{}`", path.join(" ")),
        }
    }
}

/// One node of the command tree: a leaf, a group of commands, a curried
/// group, or a group's default command.
///
/// `O` is what parsing the node yields. For everything an [`App`](crate::App)
/// mounts that is an [`Invocation`] — [`Command<Ctx>`](Command) is
/// `Node<Invocation<Ctx>>` — and for the commands of a curried group it is a
/// [`Partial`] still waiting for the group's value ([`Curried`]).
///
/// Build one with [`command`], [`group`], [`group_args`], [`curried_command`] or
/// [`external`], optionally add [`details`](Node::details), and hand it to
/// [`App::command`](crate::App::command) / [`App::commands`](crate::App::commands)
/// or to a parent group.
pub struct Node<O> {
    name: &'static str,
    help: String,
    details: Option<String>,
    footer: Option<String>,
    aliases: Vec<&'static str>,
    listing: Listing,
    kind: Kind<O>,
}

/// One node of the command tree an [`App`](crate::App) mounts: a leaf, a
/// group of commands, or an external subcommand.
pub type Command<Ctx> = Node<Invocation<Ctx>>;

/// A parsed command of a curried group, waiting for the group's value.
///
/// Opaque: it is produced by [`curried_command`] and consumed by the
/// [`group_args`] group it is mounted in, which hands it the value it parsed.
pub struct Partial<T, O>(Box<dyn FnOnce(T) -> O>);

impl<T, O> Partial<T, O> {
    /// Hand over the group's value.
    pub(crate) fn apply(self, t: T) -> O {
        (self.0)(t)
    }
}

/// A command curried over a `T`: it belongs in a [`group_args`] group parsing
/// that `T`, and cannot be mounted anywhere else.
pub type Curried<T, Ctx> = Node<Partial<T, Invocation<Ctx>>>;

/// Whether a command shows up in help, completion and suggestions. Either way
/// it parses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Listing {
    /// The default: listed everywhere.
    Listed,
    /// Set by [`Node::hide`]: typed exactly, it works; nothing advertises it.
    Hidden,
}

/// What a [`Node`] actually is. Private: a group and a leaf are one type to
/// everyone outside this module.
enum Kind<O> {
    /// A runnable command: a deferred builder for its argument parser.
    Leaf(Deferred<O>),
    /// A leaf that answers to no word: its group runs it when no sibling is
    /// named. Mounted by its group, at the group's own path.
    Default(Deferred<O>),
    /// A set of commands mounted under this one's name.
    Group(Vec<Node<O>>),
    /// A group parsing one value after its name, handed to its bound
    /// commands: a deferred builder for the whole group body.
    Curried(Deferred<O>),
}

/// A runnable command named `name`, parsed by `parser` and run by `run`.
///
/// `help` is required (ADR-001) and used twice on purpose: as the command's
/// `descr` (the heading of `app <cmd> --help`) and as its `help` (the one-line
/// entry in the parent's command list). Longer prose goes in
/// [`details`](Node::details).
///
/// `run` receives the parsed arguments and the shared context, and may return
/// anything implementing [`IntoOutcome`] — `()`, [`Success`](crate::Success),
/// or a `Result<(), E>` / `Result<Success, E>` whose error converts into a
/// [`Failure`](crate::Failure) ([`Outcome`](crate::Outcome), `io::Result`,
/// `anyhow::Result`, `eyre::Result`, …).
///
/// **A closure returning `Ok(())` needs its return type spelled.** Because the
/// error type is generic (ADR-012), `|args, ctx| { …; Ok(()) }` no longer
/// infers `E = Failure` and fails with "type annotations needed". Write
/// `|args: A, ctx: &Ctx| -> Outcome { …; Ok(()) }`. A `#[command]` fn is
/// unaffected — its return type is already written.
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
    Node::new(
        name,
        help.into(),
        Kind::Leaf(Box::new(move |at: &Mount| {
            let path = at.path.clone();
            parser
                .map(move |args| {
                    let run = run.clone();
                    Invocation::new(path.clone(), move |ctx: &Ctx| run(args, ctx).into_outcome())
                })
                .boxed()
        })),
    )
}

/// A command curried over a `T`: it runs with the value its [`group_args`] group
/// parsed, handed over as `run`'s second argument.
///
/// Everything else is as for [`command`]: `parser` parses the command's own
/// arguments (`A`), `run` returns anything [`IntoOutcome`] — and, as there, a
/// closure returning `Ok(())` must spell `-> Outcome`. The resulting
/// [`Curried`] node can only be mounted in a curried group's `bound` list.
/// Its [`Invocation::path`] skips the value: `source bandcamp sync` is
/// `["source", "sync"]`.
pub fn curried_command<T: 'static, Ctx: 'static, A: 'static, R: IntoOutcome>(
    name: &'static str,
    help: impl Into<String>,
    parser: impl Parser<A> + 'static,
    run: impl Fn(A, T, &Ctx) -> R + Clone + 'static,
) -> Curried<T, Ctx> {
    Node::new(
        name,
        help.into(),
        Kind::Leaf(Box::new(move |at: &Mount| {
            let path = at.path.clone();
            parser
                .map(move |args: A| {
                    let run = run.clone();
                    let path = path.clone();
                    Partial(Box::new(move |t: T| {
                        Invocation::new(path, move |ctx: &Ctx| run(args, t, ctx).into_outcome())
                    }))
                })
                .boxed()
        })),
    )
}

/// A group that parses one value after its name and hands it to its `bound`
/// commands: `source SOURCE-NAME sync`.
///
/// - `arg` builds the value's parser — usually `<T as Arg>::parser`, the
///   function itself, not a call. It is called more than once: once to read
///   the value's metavar for help, once for the parser.
/// - `fixed` are ordinary commands beside the value (`source list`). They are
///   matched first: a value spelled like one of them (name or alias, hidden or
///   not) is that command, never the value.
/// - `bound` are the [`Curried`] commands that run with the value.
///
/// `--help` lists the bound commands under `Commands for <METAVAR>:` — the
/// value itself is a positional item — and each bound command's usage line names the
/// value (`Usage: source SOURCE-NAME sync`). Completing a non-empty word where
/// the value goes offers the fixed commands whose names start with it, beside
/// whatever the value's own completer offers.
///
/// ```
/// # use hicli::bpaf::{positional, pure, Parser};
/// # use hicli::{cli, App};
/// let list = hicli::command("list", "list sources", pure(()), |(), _: &()| {});
/// let sync = hicli::curried_command("sync", "sync a source", pure(()), |(), name: String, _: &()| {
///     println!("syncing {name}");
/// });
/// let source = hicli::group_args(
///     "source",
///     "manage sources",
///     || positional::<String>("SOURCE-NAME"),
///     vec![list],
///     vec![sync],
/// );
/// let app = App::new(pure(())).command(source);
/// let (_, invocation) = app.parse(cli! { source bandcamp sync }).expect("parses");
/// assert_eq!(invocation.path(), ["source", "sync"]);
/// ```
///
/// # Panics
///
/// Not here — when the tree is folded: if `bound` is empty, if two children
/// (fixed or bound) share a spelling, or if any child is a
/// [default](Node::by_default) — a default command cannot live in a curried
/// group.
pub fn group_args<T: 'static, O: 'static, P: Parser<T> + 'static>(
    name: &'static str,
    help: impl Into<String>,
    arg: impl Fn() -> P + 'static,
    fixed: Vec<Node<O>>,
    bound: Vec<Node<Partial<T, O>>>,
) -> Node<O> {
    Node::new(
        name,
        help.into(),
        Kind::Curried(Box::new(move |at: &Mount| {
            mount_curried(at, arg, fixed, bound)
        })),
    )
}

/// A curried group's body, mounted at `at`: `fixed | guard T bound`.
fn mount_curried<T: 'static, O: 'static, P: Parser<T> + 'static>(
    at: &Mount,
    arg: impl Fn() -> P,
    fixed: Vec<Node<O>>,
    bound: Vec<Node<Partial<T, O>>>,
) -> Boxed<O> {
    check_spellings(
        fixed
            .iter()
            .flat_map(Node::spellings)
            .chain(bound.iter().flat_map(Node::spellings)),
        at,
    );
    let default = fixed
        .iter()
        .find(|c| c.is_default())
        .map(Node::name)
        .or_else(|| bound.iter().find(|c| c.is_default()).map(Node::name));
    if let Some(d) = default {
        panic!(
            "a default command cannot live in a curried group: `{d}` {}",
            at.place()
        );
    }
    let metavar = usage_fragment(arg());
    let reserved: Vec<&'static str> = fixed.iter().flat_map(Node::spellings).collect();
    let names = listed_names(&fixed);
    let fixed_alt = fold_named(fixed, at);
    let bound_alt = fold_named(bound, &at.bound(&metavar)).unwrap_or_else(|| {
        panic!(
            "hicli::group_args {} has no curried commands; add at least one to `bound`",
            at.place()
        )
    });
    let front = Rc::new(RefCell::new(Front::default()));
    let guard = peek(front.clone(), reserved);
    let t = arg().complete(offer_siblings(front, names));
    // The section holds the bound commands only: the value is listed with the
    // positional items, under its own doc (#20).
    let bound_alt = bound_alt.group_help(format!("Commands for {metavar}:").as_str());
    let branch = construct!(guard, t, bound_alt)
        .map(|((), t, f)| f.apply(t))
        .boxed();
    match fixed_alt {
        Some(fixed_alt) => construct!([fixed_alt, branch]).boxed(),
        None => branch,
    }
}

/// What `p`'s usage line shows for its own items — `SOURCE-NAME` — read
/// without running it, on one line however long (spike H-S2).
fn usage_fragment<T: 'static>(p: impl Parser<T> + 'static) -> String {
    let seen = RefCell::new(String::new());
    let _ = p.to_options().with_usage(|d| {
        *seen.borrow_mut() = format!("{d:10000}");
        d
    });
    seen.into_inner()
}

/// The front of a node's remaining words, as [`peek`] last saw it.
#[derive(Default)]
struct Front {
    /// The first remaining word.
    word: Option<String>,
    /// How many words remain, the first included.
    words: usize,
}

/// A hidden parser that consumes nothing: it records the node's remaining
/// words in `front` for [`offer_siblings`], and fails when the first of them
/// is `reserved` — a fixed sibling's spelling (spike S4's guard).
///
/// `bpaf` keeps only the completions of the branch that consumed the typed
/// word (spike S6), so a sibling's name can only be offered from the value's
/// own completion — which is handed the parsed value, not the word. This is
/// how the word reaches it.
fn peek(front: Rc<RefCell<Front>>, reserved: Vec<&'static str>) -> Boxed<()> {
    // Every evaluation starts from nothing seen, so a word recorded by an
    // earlier evaluation can never be read by a later one.
    let fresh = front.clone();
    let reset = bpaf::pure(()).map(move |()| *fresh.borrow_mut() = Front::default());
    let first = front.clone();
    let head = any::<String, _, _>("", move |s: String| {
        let hit = reserved.contains(&s.as_str());
        *first.borrow_mut() = Front {
            word: Some(s),
            words: 0,
        };
        hit.then_some(())
    })
    .optional()
    .guard(Option::is_none, "reserved word");
    let every = any::<String, _, _>("", move |_: String| {
        front.borrow_mut().words += 1;
        None::<()>
    })
    .anywhere()
    .optional();
    construct!(reset, head, every).map(|_| ()).hide().boxed()
}

/// A completer offering the `names` that start with the typed word — when it
/// is non-empty and the only word in the node, the one place a sibling's name
/// is still valid. An empty word gets nothing: bpaf already offers every
/// sibling there, and would list them twice.
fn offer_siblings<V>(
    front: Rc<RefCell<Front>>,
    names: Vec<Candidate>,
) -> impl Fn(&V) -> Vec<Candidate> {
    move |_| {
        let f = front.borrow();
        match (&f.word, f.words) {
            (Some(w), 1) if !w.is_empty() => names
                .iter()
                .filter(|(n, _)| n.starts_with(w.as_str()))
                .cloned()
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// The listed nodes' names and help, as completion candidates.
fn listed_names<O>(nodes: &[Node<O>]) -> Vec<Candidate> {
    nodes
        .iter()
        .filter(|c| c.is_listed())
        .map(|c| (c.name.to_string(), Some(c.help.clone())))
        .collect()
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
///
/// A group may hold one [default](Node::by_default) command, run when none of
/// its siblings is named. The same function builds a group of [`Curried`]
/// commands — what `#[group(curried = T)]` expands to — for a curried group's
/// `bound` list.
pub fn group<O: 'static>(
    name: &'static str,
    help: impl Into<String>,
    children: Vec<Node<O>>,
) -> Node<O> {
    Node::new(name, help.into(), Kind::Group(children))
}

/// A git-style external subcommand as a [`Command`].
///
/// Name and help come from the [`ExternalCommand`] manifest, so an external is
/// listed in help and completion exactly like any other command — and, as
/// everywhere else in hicli, without the binary ever being executed to find
/// out what it is.
pub fn external<Ctx: 'static>(ext: ExternalCommand) -> Command<Ctx> {
    let name = ext.name();
    let help = ext.help().to_string();
    Node::new(
        name,
        help,
        Kind::Leaf(Box::new(move |at: &Mount| {
            external_parser(ext, at.path.clone())
        })),
    )
}

impl<O> Node<O> {
    /// A listed command with no details, footer or aliases.
    fn new(name: &'static str, help: String, kind: Kind<O>) -> Self {
        Node {
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
    /// [`footer`](Node::footer).
    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Attach a footer, printed at the end of `app <cmd> --help`, after the
    /// options — **verbatim**, unlike [`details`](Node::details): every line
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

    /// Whether this is a group's default command.
    pub(crate) fn is_default(&self) -> bool {
        matches!(self.kind, Kind::Default(_))
    }

    /// Make this leaf its group's **default** command: the one that runs when
    /// none of its siblings is named (`search jazz` beside
    /// `search fact-values-for genre`).
    ///
    /// A default answers to no word. It is mounted at its group's own path —
    /// its [`Invocation::path`] is the group's — and its arguments are listed
    /// in the group's `--help` under its one-line help. A default without
    /// arguments has its one-line help shown under the group's `Usage:` line
    /// instead, which then reads `[COMMAND ...]`. A named sibling is
    /// matched first; `--` escapes a value spelled like one. A default's
    /// positionals must come after its options, by `bpaf`'s rule for any
    /// command.
    ///
    /// ```
    /// # use hicli::bpaf::{positional, pure, Parser};
    /// # use hicli::{cli, App};
    /// let facts = hicli::command("facts", "list fact types", pure(()), |(), _: &()| {});
    /// let query = hicli::command(
    ///     "query",
    ///     "search the library",
    ///     positional::<String>("QUERY").optional(),
    ///     |_q: Option<String>, _: &()| {},
    /// )
    /// .by_default();
    /// let app = App::new(pure(())).command(hicli::group("search", "search", vec![facts, query]));
    /// let (_, invocation) = app.parse(cli! { search jazz }).expect("parses");
    /// assert_eq!(invocation.path(), ["search"]);
    /// ```
    ///
    /// # Panics
    ///
    /// Here, unless this is a leaf command — a group cannot be a default. When
    /// the tree is folded: two defaults in one group, a default at the top
    /// level or in a curried group, and a default with an alias, `hide`,
    /// details or a footer — it answers to no word and has no help page.
    pub fn by_default(self) -> Self {
        let kind = match self.kind {
            Kind::Leaf(build) => Kind::Default(build),
            Kind::Default(_) => panic!("`{}` is already a default command", self.name),
            Kind::Group(_) | Kind::Curried(_) => panic!(
                "only a leaf command can be a default: `{}` is a group",
                self.name
            ),
        };
        Node { kind, ..self }
    }
}

impl<O: 'static> Node<O> {
    /// Fold this command into a `bpaf` subcommand parser mounted under
    /// `parent`.
    ///
    /// This is where the tree becomes a parser and where each invocation's path
    /// is baked in: a leaf's deferred builder is called with this command's own
    /// mount point, a group mounts its children under it first, and a curried
    /// group mounts its fixed and bound commands.
    ///
    /// # Panics
    ///
    /// Panics if this command is a group with no children, a group with two
    /// children sharing a spelling, or a default (which only its group
    /// mounts).
    pub(crate) fn mount(self, parent: &Mount) -> Boxed<O> {
        let Node {
            name,
            help,
            details,
            footer,
            aliases,
            listing,
            kind,
        } = self;
        let at = parent.child(name);
        let (inner, default): (Boxed<O>, Option<DefaultHelp>) = match kind {
            Kind::Leaf(build) | Kind::Curried(build) => (build(&at), None),
            Kind::Group(children) => {
                let folded = alternative(children, &at).unwrap_or_else(|| {
                    panic!("hicli::group `{name}` has no commands; add at least one child")
                });
                (folded.parser, folded.default)
            }
            Kind::Default(_) => panic!(
                "a default command cannot be mounted on its own: `{name}` {}",
                parent.place()
            ),
        };
        // `with_usage` asks the parser for its metadata on the spot, which is
        // what runs a default's probe (`mount_default`) and sets `has_items`.
        let mut opts = inner.to_options().descr(help.as_str()).with_usage(|d| d);
        // A default without options has no section of its own to title, so
        // its help line goes in the group's header (#19).
        let bare = default.filter(|d| !d.has_items.get());
        match (&bare, &details) {
            (Some(b), details) => {
                opts = opts.header(crate::help::default_header(&b.help, details.as_deref()));
            }
            (None, Some(d)) => opts = opts.header(d.as_str()),
            (None, None) => {}
        }
        if let Some(f) = &footer {
            opts = opts.footer(crate::help::verbatim(f));
        }
        // Last: hicli spells every usage line itself (ADR-013) — bpaf's own
        // names no binary, drops curried values from the path (spike H-S1),
        // and ends in a space when the command has no arguments.
        opts = opts.with_usage(crate::help::usage_line(at.shown.clone(), bare.is_some()));
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

    /// Mount this default command at its group's own mount point `at`,
    /// offering the group's named siblings `names` in completion. Also its
    /// help, for the group's page.
    ///
    /// # Panics
    ///
    /// Panics if this default has an alias, is hidden, or has details or a
    /// footer.
    fn mount_default(self, at: &Mount, names: Vec<Candidate>) -> (Boxed<O>, DefaultHelp) {
        let Node {
            name,
            help,
            details,
            footer,
            aliases,
            listing,
            kind,
        } = self;
        let place = at.place();
        assert!(
            aliases.is_empty(),
            "a default command answers to no word, so it takes no alias: `{name}` {place}"
        );
        assert!(
            listing == Listing::Listed,
            "a default command is never listed, so it cannot be hidden: `{name}` {place}"
        );
        assert!(
            details.is_none() && footer.is_none(),
            "a default command has no help page of its own, so it takes no details or footer: \
             `{name}` {place}"
        );
        let Kind::Default(build) = kind else {
            unreachable!("mount_default is called on defaults only")
        };
        let front = Rc::new(RefCell::new(Front::default()));
        let look = peek(front.clone(), Vec::new());
        // `group_help` titles the options in completion; the probe around it
        // records whether there are any to list, and gives the help section
        // the same title (bpaf flattens a nested section into the outer one).
        let has_items = Rc::new(Cell::new(false));
        let seen = has_items.clone();
        let title = help.clone();
        let probe = move |info: MetaInfo| {
            let mut items = Doc::default();
            items.meta(info, false);
            seen.set(!items.monochrome(false).is_empty());
            Doc::from(title.as_str())
        };
        let d = build(at)
            .complete(offer_siblings(front, names))
            .group_help(help.as_str())
            .with_group_help(probe);
        let parser = construct!(look, d).map(|((), o)| o).boxed();
        (parser, DefaultHelp { help, has_items })
    }
}

/// Panic if a spelling repeats: the second could never be reached by it.
fn check_spellings(spellings: impl Iterator<Item = &'static str>, at: &Mount) {
    let mut seen: Vec<&'static str> = Vec::new();
    for spelling in spellings {
        if seen.contains(&spelling) {
            panic!(
                "two commands answer to `{spelling}` {}; give each its own name and aliases",
                at.place()
            );
        }
        seen.push(spelling);
    }
}

/// Fold named (non-default) `children` into one alternative, in order.
fn fold_named<O: 'static>(children: Vec<Node<O>>, at: &Mount) -> Option<Boxed<O>> {
    let mut acc: Option<Boxed<O>> = None;
    for child in children {
        let next = child.mount(at);
        acc = Some(match acc {
            None => next,
            Some(acc) => construct!([acc, next]).boxed(),
        });
    }
    acc
}

/// Fold `children` into one alternative parser, mounted under `at`.
///
/// The first branch that matches wins — this is what replaces the central
/// `match`: every child folds one more command into the alternative. A
/// default child answers to no word: it is mounted at `at` itself, after its
/// named siblings, and its help is handed back for the group's page. `None`
/// when there are no children; the caller decides what that means.
///
/// # Panics
///
/// Panics if two named children share a spelling — a name or an alias —
/// because the second could never be reached by it, or if there are two
/// defaults.
pub(crate) fn alternative<O: 'static>(children: Vec<Node<O>>, at: &Mount) -> Option<Folded<O>> {
    check_spellings(
        children
            .iter()
            .filter(|c| !c.is_default())
            .flat_map(Node::spellings),
        at,
    );
    let (mut defaults, named): (Vec<_>, Vec<_>) = children.into_iter().partition(Node::is_default);
    if let [a, b, ..] = defaults.as_slice() {
        panic!(
            "two default commands {}: `{}` and `{}`; a group has at most one",
            at.place(),
            a.name,
            b.name
        );
    }
    let names = listed_names(&named);
    let named = fold_named(named, at);
    let (default, help) = match defaults.pop().map(|d| d.mount_default(at, names)) {
        Some((parser, help)) => (Some(parser), Some(help)),
        None => (None, None),
    };
    let parser = match (named, default) {
        (Some(named), Some(default)) => construct!([named, default]).boxed(),
        (named, default) => named.or(default)?,
    };
    Some(Folded {
        parser,
        default: help,
    })
}

/// The parser for an external subcommand: capture the remainder verbatim, and
/// produce an [`Invocation`] that runs the child process when — and only when —
/// it is run.
fn external_parser<Ctx: 'static>(
    ext: ExternalCommand,
    path: Vec<&'static str>,
) -> Boxed<Invocation<Ctx>> {
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
///
/// A [`Curried`] command is deliberately *not* `IntoCommand`: it needs a
/// curried group's value to run.
#[diagnostic::on_unimplemented(
    note = "if this is a #[command(curried)] handle, it belongs in a #[group(args = T)]"
)]
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

/// What `#[group(args = T)]` and `#[group(curried = T)]` pass each curried
/// child's handle through, so a child that is not curried over the group's
/// `T` is reported in the group's terms rather than as a bare type mismatch.
///
/// Implemented only for [`Curried<T, Ctx>`](Curried) itself. Not part of the
/// API: the macros' expansion is its only caller.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "this is not a command curried over `{T}` (with context `{Ctx}`)",
    note = "a #[command(curried)] in #[group(args = {T})] takes `{T}` as its first owned parameter"
)]
pub trait __CurriedOver<T, Ctx> {
    /// The curried node itself.
    fn __node(self) -> Curried<T, Ctx>;
}

impl<T, Ctx> __CurriedOver<T, Ctx> for Curried<T, Ctx> {
    fn __node(self) -> Curried<T, Ctx> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{Failure, Success};
    use bpaf::pure;

    /// Mount `c` as the only top-level command and parse `args` through it.
    fn parse<Ctx: 'static>(c: Command<Ctx>, args: &[&str]) -> Invocation<Ctx> {
        c.mount(&Mount::root(None))
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
        let _ = c.mount(&Mount::root(None));
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
        let _ = c.mount(&Mount::root(None));
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
        let _ = alternative(children, &Mount::root(None));
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

/// ADR-011: curried commands (`source SOURCE-NAME sync`) and a group's default
/// command (`search [QUERY]`), driven through the hicli API. The outcomes are
/// the ones `tests/bpaf_contract.rs` pins against raw bpaf.
#[cfg(test)]
mod curried_and_default {
    use super::*;
    use crate::outcome::Success;
    use bpaf::{positional, pure, OptionParser, ParseFailure};
    use std::cell::RefCell;

    /// The test context: every command logs what it ran with.
    type Log = RefCell<Vec<String>>;

    fn log(ctx: &Log, line: String) {
        ctx.borrow_mut().push(line);
    }

    /// `SOURCE-NAME`, completed from a fixed list (the spike's `source_name`).
    fn source_name() -> impl Parser<String> {
        positional::<String>("SOURCE-NAME")
            .help("the source")
            .complete(|s: &String| {
                ["bandcamp", "bandit", "discogs"]
                    .into_iter()
                    .filter(|c| c.starts_with(s.as_str()))
                    .map(|c| (c, None))
                    .collect::<Vec<_>>()
            })
    }

    fn list() -> Command<Log> {
        command("list", "list sources", pure(()), |(), c: &Log| {
            log(c, "list".into())
        })
    }

    fn bound() -> Vec<Curried<String, Log>> {
        vec![
            curried_command(
                "sync",
                "sync a source",
                pure(()),
                |(), s: String, c: &Log| log(c, format!("sync {s}")),
            ),
            curried_command(
                "status",
                "show a source's status",
                pure(()),
                |(), s: String, c: &Log| log(c, format!("status {s}")),
            ),
            curried_command(
                "cancel",
                "cancel a job",
                positional::<String>("ID").help("job id"),
                |id: String, s: String, c: &Log| log(c, format!("cancel {s} {id}")),
            ),
        ]
    }

    /// `source (list | SOURCE-NAME (sync | status | cancel ID))`.
    fn source() -> Command<Log> {
        group_args("source", "the node", source_name, vec![list()], bound())
    }

    /// `source SOURCE-NAME (sync | status | cancel ID)`, no fixed sibling.
    fn source_bound_only() -> Command<Log> {
        group_args("source", "the node", source_name, Vec::new(), bound())
    }

    /// `search (fact-values-for TYPE | [--artist=ARTIST] [QUERY])`.
    fn search() -> Command<Log> {
        group(
            "search",
            "the node",
            vec![
                command(
                    "fact-values-for",
                    "list values for a fact type",
                    positional::<String>("TYPE"),
                    |t: String, c: &Log| log(c, format!("facts {t}")),
                ),
                command(
                    "query",
                    "search the library",
                    query_args(),
                    |(a, q), c: &Log| log(c, format!("query {a:?} {q:?}")),
                )
                .by_default(),
            ],
        )
    }

    fn query_args() -> impl Parser<(Option<String>, Option<String>)> {
        let artist = bpaf::long("artist")
            .help("by artist")
            .argument::<String>("ARTIST")
            .optional();
        let query = positional::<String>("QUERY").help("free text").optional();
        construct!(artist, query)
    }

    /// Fold `nodes` as a top level.
    fn top<O: 'static>(nodes: Vec<Node<O>>) -> OptionParser<O> {
        alternative(nodes, &Mount::root(None))
            .expect("has commands")
            .parser
            .to_options()
    }

    /// The raw outcome of `args`: the selected path and what it logged when
    /// run, or the rendered failure.
    fn show(c: Command<Log>, args: &[&str]) -> String {
        match top(vec![c]).run_inner(args) {
            Ok(inv) => {
                let path = inv.path().join(" ");
                let ctx = Log::default();
                inv.run(&ctx).expect("runs");
                format!("Ok({path}: {})", ctx.into_inner().join(", "))
            }
            Err(e @ ParseFailure::Stderr(..)) => format!("STDERR: {}", e.unwrap_stderr()),
            Err(e @ ParseFailure::Stdout(..)) => format!("STDOUT:\n{}", e.unwrap_stdout()),
            Err(ParseFailure::Completion(s)) => format!("COMPLETION:\n{s}"),
        }
    }

    /// The raw completion payload at protocol revision `rev` (never 2: it
    /// exits the process).
    fn comp(rev: usize, c: Command<Log>, args: &[&str]) -> String {
        let flag = format!("--bpaf-complete-rev={rev}");
        let mut v = vec![flag.as_str()];
        v.extend_from_slice(args);
        show(c, &v)
    }

    // --- parsing ------------------------------------------------------------

    #[test]
    fn a_curried_command_receives_the_groups_value() {
        assert_eq!(
            show(source(), &["source", "bandcamp", "sync"]),
            "Ok(source sync: sync bandcamp)"
        );
        assert_eq!(
            show(source(), &["source", "bandcamp", "cancel", "7"]),
            "Ok(source cancel: cancel bandcamp 7)"
        );
        assert_eq!(
            show(source_bound_only(), &["source", "discogs", "status"]),
            "Ok(source status: status discogs)"
        );
    }

    #[test]
    fn a_curried_commands_path_skips_the_value() {
        let inv = top(vec![source()])
            .run_inner(&["source", "bandcamp", "sync"][..])
            .expect("parses");
        assert_eq!(inv.path(), ["source", "sync"]);
        assert_eq!(inv.name(), "sync");
    }

    #[test]
    fn a_fixed_sibling_wins_its_own_word() {
        assert_eq!(show(source(), &["source", "list"]), "Ok(source list: list)");
        // S4: the guard keeps `list` from being read as a source name.
        assert_eq!(
            show(source(), &["source", "list", "sync"]),
            "STDERR: `sync` is not expected in this context"
        );
        assert_eq!(
            show(source(), &["source", "lst"]),
            "STDERR: no such command or positional: `lst`, did you mean `list`?"
        );
    }

    #[test]
    fn a_missing_curried_command_is_a_usage_error() {
        assert_eq!(
            show(source_bound_only(), &["source", "bandcamp"]),
            "STDERR: expected `COMMAND ...`, pass `--help` for usage information"
        );
    }

    #[test]
    fn an_alias_of_a_fixed_sibling_is_reserved_too() {
        let c = group_args(
            "source",
            "the node",
            source_name,
            vec![list().alias("ls").hide()],
            bound(),
        );
        assert_eq!(show(c, &["source", "ls"]), "Ok(source list: list)");
    }

    #[test]
    fn a_default_runs_when_no_sibling_is_named() {
        assert_eq!(show(search(), &["search"]), "Ok(search: query None None)");
        assert_eq!(
            show(search(), &["search", "jazz"]),
            r#"Ok(search: query None Some("jazz"))"#
        );
        assert_eq!(
            show(search(), &["search", "--artist", "X", "jazz"]),
            r#"Ok(search: query Some("X") Some("jazz"))"#
        );
        assert_eq!(
            show(search(), &["search", "fact-values-for", "genre"]),
            "Ok(search fact-values-for: facts genre)"
        );
        // S3 (d): the named command's own error, not QUERY=fact-values-for.
        assert_eq!(
            show(search(), &["search", "fact-values-for"]),
            "STDERR: expected `TYPE`, pass `--help` for usage information"
        );
        // S3 (f): `--` is the escape hatch.
        assert_eq!(
            show(search(), &["search", "--", "fact-values-for"]),
            r#"Ok(search: query None Some("fact-values-for"))"#
        );
    }

    #[test]
    fn a_defaults_path_is_the_groups_path() {
        let inv = top(vec![search()])
            .run_inner(&["search", "jazz"][..])
            .expect("parses");
        assert_eq!(inv.path(), ["search"]);
        assert_eq!(inv.name(), "search");
    }

    #[test]
    fn a_group_of_only_a_default_parses() {
        let c = group(
            "search",
            "the node",
            vec![command("query", "search", query_args(), |(_, q), c: &Log| {
                log(c, format!("q {q:?}"))
            })
            .by_default()],
        );
        assert_eq!(show(c, &["search", "x"]), r#"Ok(search: q Some("x"))"#);
    }

    #[test]
    fn a_curried_group_nested_in_a_curried_group_parses() {
        assert_eq!(
            show(nested(), &["x", "A", "y", "B", "f"]),
            "Ok(x y f: f A B)"
        );
    }

    // --- help ---------------------------------------------------------------

    #[test]
    fn the_curried_commands_get_a_section_of_their_own() {
        assert_eq!(
            show(source(), &["source", "--help"]),
            "STDOUT:\nthe node\n\n\
             Usage: source (COMMAND ... | SOURCE-NAME COMMAND ...)\n\n\
             Commands for SOURCE-NAME:\n    \
             sync         sync a source\n    status       show a source's status\n    \
             cancel       cancel a job\n\n\
             Available positional items:\n    SOURCE-NAME  the source\n\n\
             Available options:\n    -h, --help   Prints help information\n\n\
             Available commands:\n    list         list sources\n"
        );
    }

    /// `archive (empty [-y] | <default list>)`: a default without options
    /// beside a named sibling (playr's `library archive`, #19).
    fn archive(details: Option<&str>) -> Command<Log> {
        let yes = bpaf::short('y').long("yes").help("do not ask").switch();
        let empty = command("empty", "empty the archive", yes, |y: bool, c: &Log| {
            log(c, format!("empty {y}"))
        });
        let list = command("list", "list the archive", pure(()), |(), c: &Log| {
            log(c, "list".into())
        })
        .by_default();
        let g = group("archive", "files moved aside", vec![empty, list]);
        match details {
            Some(d) => g.details(d),
            None => g,
        }
    }

    #[test]
    fn a_default_without_options_is_described_under_the_usage() {
        assert_eq!(
            show(archive(None), &["archive", "--help"]),
            "STDOUT:\nfiles moved aside\n\n\
             Usage: archive [COMMAND ...]\n\n\
             list the archive\n\n\
             Available options:\n    -h, --help  Prints help information\n\n\
             Available commands:\n    empty       empty the archive\n"
        );
        assert_eq!(show(archive(None), &["archive"]), "Ok(archive: list)");
    }

    #[test]
    fn a_default_without_options_comes_before_the_groups_details() {
        for args in [
            &["archive", "--help"][..],
            &["archive", "--help", "--help"][..],
        ] {
            let help = show(archive(Some("about")), args);
            assert!(
                help.contains("Usage: archive [COMMAND ...]\n\nlist the archive\n\nabout\n"),
                "{help}"
            );
        }
    }

    #[test]
    fn a_default_with_options_is_described_once_and_keeps_its_usage() {
        let help = show(search(), &["search", "--help"]);
        assert_eq!(help.matches("search the library").count(), 1, "{help}");
        assert_eq!(
            usage_line(&help),
            "Usage: search (COMMAND ... | [--artist=ARTIST] [QUERY])",
            "{help}"
        );
    }

    #[test]
    fn a_group_of_only_a_bare_default_has_an_unbracketed_usage() {
        let c = group(
            "archive",
            "files moved aside",
            vec![command("list", "list the archive", pure(()), |(), _: &Log| {}).by_default()],
        );
        let help = show(c, &["archive", "--help"]);
        assert_eq!(usage_line(&help), "Usage: archive", "{help}");
        assert!(help.contains("\nlist the archive\n"), "{help}");
    }

    #[test]
    fn a_curried_commands_usage_names_the_value() {
        assert_eq!(
            show(source(), &["source", "bandcamp", "sync", "--help"]),
            "STDOUT:\nsync a source\n\nUsage: source SOURCE-NAME sync\n\n\
             Available options:\n    -h, --help  Prints help information\n"
        );
        assert_eq!(
            show(source(), &["source", "bandcamp", "cancel", "--help"]),
            "STDOUT:\ncancel a job\n\nUsage: source SOURCE-NAME cancel ID\n\n\
             Available positional items:\n    ID          job id\n\n\
             Available options:\n    -h, --help  Prints help information\n"
        );
        // A fixed sibling's usage names no value.
        assert_eq!(
            show(source(), &["source", "list", "--help"]),
            "STDOUT:\nlist sources\n\nUsage: source list\n\n\
             Available options:\n    -h, --help  Prints help information\n"
        );
    }

    /// `x A (y B f)`: a curried group inside a curried group.
    fn nested() -> Command<Log> {
        type Inner = Partial<String, Invocation<Log>>;
        let f: Node<Partial<String, Inner>> = Node::new(
            "f",
            "f leaf".into(),
            Kind::Leaf(Box::new(|at: &Mount| {
                let path = at.path.clone();
                pure(())
                    .map(move |()| {
                        let path = path.clone();
                        Partial(Box::new(move |b: String| {
                            Partial(Box::new(move |a: String| {
                                Invocation::new(path, move |c: &Log| {
                                    log(c, format!("f {a} {b}"));
                                    Ok(Success::OK)
                                })
                            }))
                        }))
                    })
                    .boxed()
            })),
        );
        let y: Node<Inner> = group_args(
            "y",
            "y node",
            || positional::<String>("B"),
            Vec::new(),
            vec![f],
        );
        group_args(
            "x",
            "x node",
            || positional::<String>("A"),
            Vec::new(),
            vec![y],
        )
    }

    /// The `Usage:` line of a help page.
    fn usage_line(out: &str) -> &str {
        out.lines()
            .find(|l| l.starts_with("Usage: "))
            .unwrap_or("no usage line")
    }

    #[test]
    fn a_nested_curried_usage_names_every_value() {
        let help = show(nested(), &["x", "A", "y", "--help"]);
        assert_eq!(usage_line(&help), "Usage: x A y B COMMAND ...", "{help}");
        let help = show(nested(), &["x", "A", "y", "B", "f", "--help"]);
        assert_eq!(usage_line(&help), "Usage: x A y B f", "{help}");
    }

    #[test]
    fn a_defaults_options_are_listed_under_its_help_line() {
        let help = show(search(), &["search", "--help"]);
        assert_eq!(
            usage_line(&help),
            "Usage: search (COMMAND ... | [--artist=ARTIST] [QUERY])",
            "{help}"
        );
        assert!(help.contains("search the library\n"), "{help}");
        assert!(
            help.contains("Available commands:\n    fact-values-for"),
            "{help}"
        );
    }

    // --- completion -----------------------------------------------------------

    #[test]
    fn a_typed_prefix_of_a_fixed_sibling_completes_at_every_revision() {
        assert_eq!(comp(1, source(), &["source", "l"]), "COMPLETION:\nlist\n");
        assert_eq!(
            comp(7, source(), &["source", "l"]),
            "COMPLETION:\ncompadd -- 'list'\n"
        );
        assert_eq!(
            comp(8, source(), &["source", "l"]),
            "COMPLETION:\nCOMPREPLY+=( 'list' )\n\n"
        );
        assert_eq!(
            comp(9, source(), &["source", "l"]),
            "COMPLETION:\nlist\tlist sources\n"
        );
    }

    #[test]
    fn a_hidden_fixed_sibling_is_never_offered() {
        let c = group_args(
            "source",
            "the node",
            source_name,
            vec![list().hide()],
            bound(),
        );
        assert_eq!(comp(1, c, &["source", "l"]), "COMPLETION:\n\n");
    }

    #[test]
    fn the_values_completer_still_completes_values() {
        assert_eq!(
            comp(1, source(), &["source", "b"]),
            "COMPLETION:\n\tthe source\nbandcamp\nbandit\n"
        );
    }

    #[test]
    fn after_the_value_only_curried_commands_are_offered() {
        assert_eq!(
            comp(1, source(), &["source", "bandcamp", ""]),
            "COMPLETION:\nsync\tsync a source\nstatus\tshow a source's status\n\
             cancel\tcancel a job\n"
        );
        assert_eq!(
            comp(9, source(), &["source", "bandcamp", ""]),
            "COMPLETION:\ncancel\tcancel a job\nstatus\tshow a source's status\n\
             sync\tsync a source\n"
        );
        let zsh = comp(7, source(), &["source", "bandcamp", ""]);
        for c in ["'sync'", "'status'", "'cancel'"] {
            assert!(zsh.contains(c), "{zsh}");
        }
        let bash = comp(8, source(), &["source", "bandcamp", ""]);
        for c in ["sync ", "status ", "cancel "] {
            assert!(bash.contains(c), "{bash}");
        }
    }

    #[test]
    fn the_empty_word_offers_each_fixed_sibling_once() {
        let out = comp(1, source(), &["source", ""]);
        assert_eq!(out.matches("list\t").count(), 1, "{out}");
        assert!(out.contains("bandcamp\n"), "{out}");
    }

    #[test]
    fn a_typed_prefix_of_a_named_sibling_completes_past_the_default() {
        assert_eq!(
            comp(1, search(), &["search", "f"]),
            "COMPLETION:\nfact-values-for\n"
        );
        assert_eq!(
            comp(7, search(), &["search", "f"]),
            "COMPLETION:\ncompadd -- 'fact-values-for'\n"
        );
        assert_eq!(
            comp(8, search(), &["search", "f"]),
            "COMPLETION:\nCOMPREPLY+=( 'fact-values-for' )\n\n"
        );
        assert_eq!(
            comp(9, search(), &["search", "f"]),
            "COMPLETION:\nfact-values-for\tlist values for a fact type\n"
        );
        // Not after an option: the named command is no longer reachable there.
        let out = comp(1, search(), &["search", "--artist", "f"]);
        assert!(!out.contains("fact-values-for"), "{out}");
        let out = comp(1, search(), &["search", "jazz", "f"]);
        assert!(!out.contains("fact-values-for"), "{out}");
    }

    #[test]
    fn the_empty_word_offers_a_named_sibling_once() {
        let out = comp(1, search(), &["search", ""]);
        assert_eq!(out.matches("fact-values-for").count(), 1, "{out}");
    }

    #[test]
    fn a_default_with_no_positional_offers_nothing_extra() {
        let c = group(
            "search",
            "the node",
            vec![
                command("facts", "facts", pure(()), |(), _: &Log| {}),
                command(
                    "query",
                    "search",
                    bpaf::long("artist").argument::<String>("A").optional(),
                    |_, _: &Log| {},
                )
                .by_default(),
            ],
        );
        let out = comp(1, c, &["search", "--artist", "f"]);
        assert!(!out.contains("facts"), "{out}");
    }

    /// C-S4's placeholder hazard, pinned: when neither the value completer nor
    /// a fixed sibling matches a typed word, bpaf keeps the metavar as the lone
    /// candidate, and zsh/bash are handed it. See the module docs.
    #[test]
    fn an_unmatched_word_still_hands_zsh_and_bash_the_metavar() {
        assert_eq!(comp(1, source(), &["source", "zz"]), "COMPLETION:\n\n");
        assert_eq!(
            comp(7, source(), &["source", "zz"]),
            "COMPLETION:\ncompadd -- 'SOURCE-NAME'\ncompadd ''\n"
        );
        assert_eq!(
            comp(8, source(), &["source", "zz"]),
            "COMPLETION:\nCOMPREPLY+=( 'SOURCE-NAME' '')\n"
        );
        assert_eq!(comp(9, source(), &["source", "zz"]), "COMPLETION:\n");
    }

    // --- panics -------------------------------------------------------------

    #[test]
    #[should_panic(expected = "two commands answer to `list` in hicli::group `source`")]
    fn a_fixed_and_a_curried_command_cannot_share_a_spelling() {
        let clash = curried_command("list", "x", pure(()), |(), _: String, _: &Log| {});
        let c = group_args("source", "the node", source_name, vec![list()], vec![clash]);
        let _ = c.mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "a default command cannot live in a curried group")]
    fn a_fixed_default_cannot_live_in_a_curried_group() {
        let c = group_args(
            "source",
            "the node",
            source_name,
            vec![list().by_default()],
            bound(),
        );
        let _ = c.mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "a default command cannot live in a curried group")]
    fn a_curried_default_cannot_live_in_a_curried_group() {
        let d = curried_command("sync", "x", pure(()), |(), _: String, _: &Log| {}).by_default();
        let c = group_args("source", "the node", source_name, vec![list()], vec![d]);
        let _ = c.mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "two default commands in hicli::group `search`")]
    fn a_group_has_at_most_one_default() {
        let d = || command("q", "q", pure(()), |(), _: &Log| {}).by_default();
        let c = group("search", "s", vec![d(), d()]);
        let _ = c.mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "only a leaf command can be a default")]
    fn a_group_cannot_be_a_default() {
        let _ = search().by_default();
    }

    #[test]
    #[should_panic(expected = "only a leaf command can be a default")]
    fn a_curried_group_cannot_be_a_default() {
        let _ = source().by_default();
    }

    fn with_default(d: Command<Log>) -> Command<Log> {
        group("search", "s", vec![list(), d])
    }

    fn default_leaf() -> Command<Log> {
        command("q", "q", pure(()), |(), _: &Log| {}).by_default()
    }

    #[test]
    #[should_panic(expected = "a default command answers to no word")]
    fn a_default_takes_no_alias() {
        let _ = with_default(default_leaf().alias("x")).mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "a default command is never listed")]
    fn a_default_cannot_be_hidden() {
        let _ = with_default(default_leaf().hide()).mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "a default command has no help page of its own")]
    fn a_default_takes_no_details() {
        let _ = with_default(default_leaf().details("d")).mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "a default command has no help page of its own")]
    fn a_default_takes_no_footer() {
        let _ = with_default(default_leaf().footer("f")).mount(&Mount::root(None));
    }

    #[test]
    #[should_panic(expected = "a default command cannot be at the top level")]
    fn a_default_cannot_be_at_the_top_level() {
        let _ = crate::App::new(pure(()))
            .command(default_leaf())
            .into_parser();
    }

    #[test]
    fn a_curried_command_is_not_into_command() {
        // Structural: `Curried` is a different type from `Command`, so it can
        // only be mounted in a curried group. The `on_unimplemented` note is
        // compile-time and not testable here.
        let c: Curried<String, Log> =
            curried_command("sync", "s", pure(()), |(), _: String, _: &Log| {});
        assert_eq!(c.name(), "sync");
    }
}
