//! The composition builder: [`App`] and exit-code handling.

use crate::command::{alternative, Command, IntoCommand, Mount};
use crate::help::{self, usage_line};
use crate::invocation::Invocation;
use crate::outcome::{Failure, Success};
use bpaf::{construct, OptionParser, ParseFailure, Parser};
use std::process::ExitCode;

/// The composition builder.
///
/// `G` is the global-options type; `Ctx` is the shared context handed to every
/// command. Build the tree with [`App::command`] / [`App::commands`], then either
/// [`App::run`] it or, in tests, drive it through [`App::parse`] /
/// [`App::into_parser`] / [`App::run_with_args`].
///
/// Note: `new` takes any `impl Parser<G>` (not `OptionParser<G>`); a plain
/// `#[derive(Bpaf)]` on the globals struct yields exactly such a parser, and
/// globals must compose *as a parser* with the command at the top level.
pub struct App<G, Ctx> {
    globals: Box<dyn Parser<G>>,
    /// The top-level commands, in registration order. Kept as *data* until
    /// [`App::into_parser`] folds them, because a command's parser cannot be
    /// built before its mount point is known.
    commands: Vec<Command<Ctx>>,
    /// The app's own description, above `Usage:` in top-level `--help`.
    about: Option<String>,
    /// The version `--version` / `-V` prints.
    version: Option<String>,
    /// The program name every usage line starts with — display only, never
    /// handed to `bpaf` (ADR-013).
    bin_name: Option<String>,
}

impl<G: 'static, Ctx: 'static> App<G, Ctx> {
    /// Start an app from a global-options parser.
    pub fn new(globals: impl Parser<G> + 'static) -> Self {
        App {
            globals: globals.boxed(),
            commands: Vec::new(),
            about: None,
            version: None,
            bin_name: None,
        }
    }

    /// Describe the app: printed at the top of `app --help`, above `Usage:`.
    ///
    /// A command's description lives in its doc comment; the app's lives here,
    /// on the composition root, because the app is not an item with a doc
    /// comment of its own.
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    /// Give the app a version: `app --version` and `app -V` print
    /// `Version: <version>` to stdout and exit `0`, and both flags are listed
    /// in top-level `--help`.
    ///
    /// Once a version is set, **`-V` and `--version` are reserved** at the top
    /// level: a global option spelled the same way silently shadows the
    /// version flag instead of failing, so don't declare one.
    ///
    /// ```
    /// # use hicli::{App, cli, exit_code_for};
    /// # use hicli::bpaf::pure;
    /// # fn app() -> App<(), ()> {
    /// #     App::new(pure(()))
    /// #         .version("1.2.3")
    /// #         .command(hicli::command("status", "show status", pure(()), |(), _: &()| {}))
    /// # }
    /// // usually `.version(env!("CARGO_PKG_VERSION"))`
    /// let e = app().parse(cli! { --version }).err().expect("--version short-circuits");
    /// assert_eq!(exit_code_for(&e), 0);
    /// assert_eq!(e.unwrap_stdout(), "Version: 1.2.3\n");
    /// assert_eq!(app().run_with_args(cli! { -V }, |()| Ok(())), 0);
    /// ```
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Name the program: every usage line starts with it,
    /// `Usage: playr playback stop`.
    ///
    /// [`App::run`] defaults it to the file name of `argv[0]` — the name the
    /// user typed, a symlink's included. The test seams ([`App::parse`],
    /// [`App::run_with_args`]) read no `argv[0]` and show no name unless this
    /// is set, so set it in a snapshot test to see what `run` prints.
    ///
    /// Display only: the name is never handed to `bpaf` (ADR-009 §3,
    /// ADR-013), so it changes no parsing and no completion.
    ///
    /// ```
    /// # use hicli::{App, cli};
    /// # use hicli::bpaf::pure;
    /// let app = App::new(pure(()))
    ///     .bin_name("playr")
    ///     .command(hicli::command("stop", "stop playback", pure(()), |(), _: &()| {}));
    /// let help = app.parse(cli! { stop --help }).err().expect("--help short-circuits");
    /// assert!(help.unwrap_stdout().contains("Usage: playr stop\n"));
    /// ```
    pub fn bin_name(mut self, name: impl Into<String>) -> Self {
        self.bin_name = Some(name.into());
        self
    }

    /// Add one top-level command.
    ///
    /// Takes anything [`IntoCommand`]: a [`Command`] built with
    /// [`command`](fn@crate::command) / [`group`](fn@crate::group) /
    /// [`external`](crate::external), or a zero-argument function returning one.
    pub fn command(mut self, c: impl IntoCommand<Ctx>) -> Self {
        self.commands.push(c.into_command());
        self
    }

    /// Add several top-level commands at once — what `commands![…]` produces.
    pub fn commands(mut self, cs: impl IntoIterator<Item = Command<Ctx>>) -> Self {
        self.commands.extend(cs);
        self
    }

    /// Fold the tree: the **top-level** command registry (for the suggestion
    /// gap-closer) and the parser.
    ///
    /// The registry is *derived* here rather than accumulated at
    /// registration, so there is one list and no way for it to disagree with
    /// the tree. Deliberately top-level only (see ADR
    /// "suggestion-registry-is-top-level-only"): nested command names live in
    /// their own subtree and are covered by `bpaf`'s own suggestions, which
    /// know which subtree the user is actually in. Help and completion are
    /// rendered by `bpaf` from each command's own `(name, help)` pair, not from
    /// this list.
    ///
    /// # Panics
    ///
    /// Panics if no command was registered, if a registered group is empty,
    /// if two siblings share a name or alias, or if a top-level command is a
    /// [default](crate::Node::by_default).
    fn fold(self) -> (Registry, OptionParser<(G, Invocation<Ctx>)>) {
        let App {
            globals,
            commands,
            about,
            version,
            bin_name,
        } = self;
        if let Some(d) = commands.iter().find(|c| c.is_default()) {
            panic!(
                "a default command cannot be at the top level: `{}` runs when no sibling is \
                 named, and only a group has siblings to name; put it in a group",
                d.name()
            );
        }
        let registry = Registry {
            known: commands.iter().flat_map(Command::spellings).collect(),
            listed: commands
                .iter()
                .filter(|c| c.is_listed())
                .map(Command::name)
                .collect(),
        };
        // A top-level default panicked above, so the fold has no default's
        // help to place.
        let tree = alternative(commands, &Mount::root(bin_name.as_deref()))
            .expect("hicli::App has no commands; add at least one via .command/.commands")
            .parser;
        // hicli spells the usage line itself (ADR-013): the name is display
        // only, never `Args::set_name` (ADR-009 §3).
        let mut parser = construct!(globals, tree)
            .to_options()
            .with_usage(usage_line(bin_name.into_iter().collect(), false));
        if let Some(about) = &about {
            parser = parser.descr(about.as_str());
        }
        if let Some(version) = &version {
            parser = parser.version(version.as_str());
        }
        // bpaf checks that every positional comes after every option only when
        // help renders — and then panics, in any build. A debug build renders
        // it once here, so a broken order fails where the tree is built
        // instead of at a user's `--help`. (Not `check_invariants`: it prints
        // to stdout.)
        #[cfg(debug_assertions)]
        {
            let _ = parser.run_inner(&["--help"][..]);
        }
        (registry, parser)
    }

    /// The top-level `bpaf` parser yielding `(globals, selected invocation)`.
    ///
    /// This is a test seam: it lets a test render help/usage and inspect the
    /// selected command without any process side effects.
    ///
    /// # Panics
    ///
    /// Panics if no command was registered — an app must have at least one
    /// [`command`](App::command) — or if a registered group is empty.
    pub fn into_parser(self) -> OptionParser<(G, Invocation<Ctx>)> {
        self.fold().1
    }

    /// Parse `args` in-process, returning the raw `bpaf` result.
    ///
    /// `args` is anything iterable of string-likes, so it plugs straight into
    /// [`cli!`](crate::cli):
    /// `app().parse(cli! { event create x "My Event" })`.
    ///
    /// # Panics
    ///
    /// Routes through [`App::into_parser`], so it panics if no command was
    /// registered.
    pub fn parse<I, S>(self, args: I) -> Result<(G, Invocation<Ctx>), ParseFailure>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let owned: Vec<String> = args.into_iter().map(Into::into).collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        self.into_parser().run_inner(refs.as_slice())
    }

    /// Complete `args` in-process, returning the candidate **values**.
    ///
    /// This is the completion-level test seam, and a sibling of [`App::parse`]
    /// and [`App::run_with_args`]: it appends nothing a user has to know. It
    /// prepends `bpaf`'s completion request to `args`, folds the same tree
    /// [`App::into_parser`] folds, and returns one `String` per offered
    /// candidate — the value the shell would insert, with the description (which
    /// the wire protocol sends beside it) dropped. `args` is anything iterable
    /// of string-likes, so it plugs straight into [`cli!`](crate::cli); the last
    /// element is the word being completed, which is `""` at a fresh prompt.
    ///
    /// ```
    /// # use hicli::{cli, App};
    /// # use hicli::bpaf::pure;
    /// # struct Ctx;
    /// /// show the status
    /// #[hicli::command]
    /// fn status(ctx: &Ctx) {
    ///     let _ = ctx;
    /// }
    ///
    /// fn main() {
    ///     let app: App<(), Ctx> = hicli::app! { globals: pure(()), commands: [status] };
    ///     assert_eq!(app.complete(cli! { "" }), ["status"]);
    /// }
    /// ```
    ///
    /// # Protocol revision 1, pinned
    ///
    /// `--bpaf-complete-rev=1` is deliberate and load-bearing: **revision 2
    /// asked for through `run_inner` terminates the process**, which would take
    /// the test runner with it. Revision 1 short-circuits as a
    /// `ParseFailure::Completion` payload instead, which is a value a test can
    /// look at. Do not "upgrade" this.
    ///
    /// # What it cannot show
    ///
    /// A shell-directive completion ([`complete_files`](crate::complete_files) /
    /// [`complete_dirs`](crate::complete_dirs)) yields **no** values here: it is
    /// an instruction to the shell, not a list, and revision 1 has no way to
    /// spell one. An empty result therefore means "nothing applicable *or* the
    /// shell's job" — assert the parse, not the completion, for those.
    ///
    /// # Panics
    ///
    /// Routes through [`App::into_parser`], so it panics if no command was
    /// registered.
    pub fn complete<I, S>(self, args: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let owned: Vec<String> = std::iter::once(COMPLETE_REV_1.to_string())
            .chain(args.into_iter().map(Into::into))
            .collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        match self.into_parser().run_inner(refs.as_slice()) {
            Err(ParseFailure::Completion(out)) => candidate_values(&out),
            // Anything else is not a completion at all — a usage error from a
            // bumped protocol revision, or a parse that somehow succeeded — and
            // has no candidates to report.
            _ => Vec::new(),
        }
    }

    /// Run the app against `std::env::args()`.
    ///
    /// Steps: parse (remapping `bpaf`'s exit codes — usage `2`, help/version
    /// `0`, appending a `did you mean` suggestion when `bpaf` produced none),
    /// build the context via `mk_ctx`, then run the selected command. A
    /// [`Success`] is carried out as its code and prints nothing; a
    /// [`Failure`]'s message goes to stderr and its code — `2` for usage, `1`
    /// for runtime — comes back. `hicli` never calls `std::process::exit`
    /// itself: the code is *returned*, as a `std::process::ExitCode` that
    /// `main` can hand straight back.
    ///
    /// Unless [`App::bin_name`] was set, the program name usage lines start
    /// with is the file name of `argv[0]` — what the user typed, a symlink's
    /// name included. It is display only (ADR-013).
    ///
    /// # Output
    ///
    /// Everything is printed by `bpaf`'s `ParseFailure::print_message`
    /// (ADR-015), so:
    ///
    /// - A usage error **and** a [`Failure`]'s message go to stderr prefixed
    ///   `Error: `. The `did you mean` hint is a line of its own below
    ///   `bpaf`'s message. A `Failure`'s message is printed line for line as
    ///   written; a line longer than 100 columns wraps.
    /// - Help and `--version` go to stdout, followed by a blank line.
    /// - With the default `color` feature, help and errors are styled (bold,
    ///   underline, the `Error: ` prefix bold red) — but only when stdout
    ///   **and** stderr are both terminals, so `app --help | less` and
    ///   `app --help 2>/dev/null` print plain text. The environment is the
    ///   only control (`NO_COLOR`, `FORCE_COLOR`, … — see the crate docs'
    ///   *Features*); there is no `--color` flag.
    ///
    /// [`App::parse`] returns the same message without the `Error: ` prefix,
    /// never coloured.
    ///
    /// Otherwise equivalent to `ExitCode::from(run_with_args(std::env::args().skip(1),
    /// mk_ctx))`; use [`App::run_with_args`] in tests, since `ExitCode` is not
    /// comparable and a test seam must be assertable.
    ///
    /// # Panics
    ///
    /// Routes through [`App::into_parser`], so it panics if no command was
    /// registered.
    pub fn run(mut self, mk_ctx: impl FnOnce(G) -> Result<Ctx, Failure>) -> ExitCode {
        let mut args = std::env::args();
        let argv0 = args.next();
        if self.bin_name.is_none() {
            self.bin_name = argv0.as_deref().and_then(program_name);
        }
        ExitCode::from(self.run_with_args(args, mk_ctx))
    }

    /// Run the app against an explicit argument vector, returning the process
    /// exit code.
    ///
    /// This is the run-level test seam: it is exactly [`App::run`] minus the
    /// `std::env::args()` read and the `ExitCode` wrapper, so a test can drive
    /// the whole pipeline — parse, exit-code remap, the `did you mean`
    /// gap-closer, `mk_ctx`, dispatch and [`Success`]/[`Failure`]
    /// propagation — in-process. `args` is anything iterable of string-likes,
    /// so it plugs straight into [`cli!`](crate::cli):
    /// `app().run_with_args(cli! { status }, mk_ctx)`.
    ///
    /// `args` is the argument vector **without** the program name, matching
    /// `std::env::args().skip(1)`.
    ///
    /// Only the returned code is assertable: help, usage errors and a
    /// [`Failure`]'s message still go to the process's real stdout/stderr —
    /// printed exactly as [`App::run`] prints them, `Error: ` prefix and
    /// colour included — which this method does not capture or redirect.
    /// Assert rendered text through [`App::parse`] (which returns the
    /// `ParseFailure` instead of printing it, plain).
    ///
    /// ```
    /// # use hicli::{App, cli};
    /// # use hicli::bpaf::{positional, pure, Parser};
    /// # struct Ctx { home: String }
    /// # fn greet() -> hicli::Command<Ctx> {
    /// #     let who = positional::<String>("WHO");
    /// #     hicli::command("greet", "greet someone", who, |who: String, ctx: &Ctx| {
    /// #         println!("[{}] hello {who}", ctx.home);
    /// #     })
    /// # }
    /// # fn app() -> App<(), Ctx> {
    /// #     App::new(pure(())).command(greet())
    /// # }
    /// // the whole run path, in-process: parse -> mk_ctx -> run -> exit code
    /// assert_eq!(app().run_with_args(cli! { greet world }, |_| Ok(Ctx { home: ".".into() })), 0);
    /// // a missing positional is a usage error, remapped from bpaf's 1 to 2
    /// assert_eq!(app().run_with_args(cli! { greet }, |_| Ok(Ctx { home: ".".into() })), 2);
    /// ```
    ///
    /// # Panics
    ///
    /// Folds the same tree [`App::into_parser`] does, so it panics if no
    /// command was registered.
    pub fn run_with_args<I, S>(self, args: I, mk_ctx: impl FnOnce(G) -> Result<Ctx, Failure>) -> u8
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (registry, parser) = self.fold();
        match parser.run_inner(refs.as_slice()) {
            Ok((globals, invocation)) => match mk_ctx(globals) {
                Ok(ctx) => report_outcome(invocation.run(&ctx)),
                Err(e) => report_outcome(Err(e)),
            },
            Err(failure) => report_failure(failure, &registry, &args),
        }
    }
}

/// The top-level spellings the suggestion gap-closer reads (ADR-002, ADR-008).
///
/// Two lists, because a spelling can be *recognised* without being
/// *advertised*: an alias or a hidden command typed exactly is a match and
/// needs no hint, but only a listed command's canonical name is ever
/// suggested — a hint must not reveal what `--help` does not show.
struct Registry {
    /// Every spelling of every top-level command — names and aliases, hidden
    /// or not. An exact match on any of these is never answered with a hint.
    known: Vec<&'static str>,
    /// The canonical names of the listed (non-hidden) top-level commands: the
    /// only suggestion candidates.
    listed: Vec<&'static str>,
}

/// The program name a usage line shows for `argv0`: its file name, so
/// `/usr/bin/playr` and `./target/debug/playr` are both `playr`. `None` when
/// there is no file name to show.
fn program_name(argv0: &str) -> Option<String> {
    std::path::Path::new(argv0)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
}

/// `bpaf`'s completion request, at the only revision that is safe in-process.
///
/// Revision 2 asked for through `run_inner` **exits the process** (verified
/// against bpaf 0.9.28), so a test seam that used it would kill the test runner
/// instead of returning. Revision 1 comes back as a
/// `ParseFailure::Completion` payload.
pub(crate) const COMPLETE_REV_1: &str = "--bpaf-complete-rev=1";

/// The candidate values in a revision-1 completion payload.
///
/// The wire format (verified against bpaf 0.9.27) has **no header line**: it
/// is one item per line, `"<value>\t<description>"` or just `"<value>"`.
///
/// - A metavariable placeholder — what a value completer contributes beside
///   its candidates — is an item with an **empty** value (`"\tthe source"`),
///   and is not a candidate.
/// - A lone item is printed without its description (a shell with one answer
///   inserts it rather than showing a menu).
/// - Nothing applicable is the empty string, or a lone empty line.
///
/// So every line's value up to its first tab is a candidate, unless it is
/// empty.
pub(crate) fn candidate_values(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.split('\t').next())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect()
}

/// The remapped process exit code for a `bpaf` parse failure.
///
/// `bpaf` uses `1` for usage errors and `0` for help/version/completion; this
/// remaps usage errors to `2` (leaving help/version/completion at `0`).
pub fn exit_code_for(f: &ParseFailure) -> u8 {
    if matches!(f, ParseFailure::Stderr(..)) {
        2
    } else {
        0
    }
}

/// Print a parse failure to the right stream, appending a suggestion when
/// `bpaf` produced none, and return the remapped exit code.
///
/// Printed by `bpaf`'s `print_message` — its one coloured path (ADR-015) —
/// which also covers shell completion: it prints a `Completion` payload with
/// `print!`, as is.
fn report_failure(failure: ParseFailure, registry: &Registry, args: &[String]) -> u8 {
    let code = exit_code_for(&failure);
    suggested(failure, registry, args).print_message(help::WIDTH);
    code
}

/// Print a command's outcome and return its process exit code.
///
/// A [`Failure`]'s message goes to stderr the way a usage error does —
/// through `bpaf`'s `print_message`, so it reads `Error: <message>`, coloured
/// on a terminal (ADR-015) — and line for line as written (`help::verbatim`).
/// A [`Success`] prints nothing at all, not even a blank line. It is produced
/// by a command that has already said whatever there was to say (an external
/// subcommand's child writes its own stdout/stderr), so anything hicli added
/// would be noise.
fn report_outcome(outcome: Result<Success, Failure>) -> u8 {
    match outcome {
        Ok(s) => s.get(),
        Err(e) => {
            ParseFailure::Stderr(help::verbatim(e.message())).print_message(help::WIDTH);
            e.code()
        }
    }
}

/// A usage error with a `did you mean` line appended when `bpaf` produced no
/// suggestion of its own; every other failure — help, version, completion —
/// untouched.
fn suggested(failure: ParseFailure, registry: &Registry, args: &[String]) -> ParseFailure {
    match failure {
        ParseFailure::Stderr(doc) => {
            if doc.monochrome(true).to_lowercase().contains("did you mean") {
                return ParseFailure::Stderr(doc);
            }
            match suggest(registry, args) {
                Some(name) => ParseFailure::Stderr(help::with_suggestion(&doc, &name)),
                None => ParseFailure::Stderr(doc),
            }
        }
        other => other,
    }
}

/// The suggestion gap-closer: a Levenshtein pass (edit distance 1..=2) over the
/// listed top-level command names, applied to the **command position only**.
///
/// A word that is any known spelling — an alias, a hidden command's name — is
/// an exact match and gets no hint; only a listed command's canonical name is
/// ever suggested.
///
/// Scope (see ADR "suggestion-registry-is-top-level-only"): only the first argv
/// word that is neither a flag nor a flag's value is considered. Nested commands
/// and flags are covered by bpaf's own suggestions, and a top-level registry
/// cannot speak for a subtree.
///
/// Because hicli does not know which globals take a value, a bare word
/// directly after a dashed token is assumed to be that option's value and is
/// skipped. That trades a false negative (`app -v stauts` yields no hint) for
/// never emitting a false positive on an option's value — the strictly safer
/// direction for an error message.
fn suggest(registry: &Registry, args: &[String]) -> Option<String> {
    let mut prev_was_flag = false;
    for tok in args {
        if tok == "--" {
            // Everything after `--` is verbatim, never a command.
            return None;
        }
        if tok.starts_with('-') {
            // An inline `--home=x` carries its own value.
            prev_was_flag = !tok.contains('=');
            continue;
        }
        if prev_was_flag {
            prev_was_flag = false;
            continue;
        }
        // This is the command position.
        if registry.known.contains(&tok.as_str()) {
            return None;
        }
        return closest(tok, registry.listed.iter().copied()).map(str::to_string);
    }
    None
}

/// The name in `names` closest to `word` — one or two edits away (Levenshtein)
/// — or `None` if none is. The nearest wins; a tie goes to the earlier name.
/// An exact match is no suggestion.
pub(crate) fn closest<'a>(word: &str, names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for name in names {
        let d = strsim::levenshtein(word, name);
        if (1..=2).contains(&d) && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, name));
        }
    }
    best.map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `["a", "b"]` as the `&[String]` `suggest` takes.
    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    /// A registry in which every name is listed and nothing else is known.
    fn all(names: &[&'static str]) -> Registry {
        Registry {
            known: names.to_vec(),
            listed: names.to_vec(),
        }
    }

    /// The registry an app with these commands folds to.
    fn registry_of(commands: Vec<Command<()>>) -> Registry {
        App::new(bpaf::pure(())).commands(commands).fold().0
    }

    /// A do-nothing top-level command.
    fn cmd(name: &'static str) -> Command<()> {
        crate::command(name, "help", bpaf::pure(()), |(), _: &()| {})
    }

    #[test]
    fn an_exact_alias_needs_no_suggestion() {
        let r = registry_of(vec![cmd("status").alias("st"), cmd("sync")]);
        assert_eq!(suggest(&r, &argv(&["st"])), None);
        // … known, but never a candidate: only canonical names are offered.
        assert_eq!(r.known, ["status", "st", "sync"]);
        assert_eq!(r.listed, ["status", "sync"]);
        assert_eq!(suggest(&r, &argv(&["sx"])), None);
    }

    #[test]
    fn a_hidden_name_is_an_exact_match_and_never_a_suggestion() {
        let r = registry_of(vec![cmd("status"), cmd("secret").hide()]);
        assert_eq!(suggest(&r, &argv(&["secret"])), None);
        assert_eq!(suggest(&r, &argv(&["secrt"])), None);
        assert_eq!(r.known, ["status", "secret"]);
        assert_eq!(r.listed, ["status"]);
    }

    #[test]
    fn suggest_matches_the_command_position() {
        let names = ["event", "status", "config"];
        assert_eq!(
            suggest(&all(&names), &argv(&["stauts"])),
            Some("status".to_string())
        );
        assert_eq!(
            suggest(&all(&names), &argv(&["evnt", "x"])),
            Some("event".to_string())
        );
    }

    #[test]
    fn suggest_ignores_tokens_after_a_recognised_command() {
        let names = ["event", "status", "config"];
        assert_eq!(suggest(&all(&names), &argv(&["status", "evnt"])), None);
        assert_eq!(suggest(&all(&names), &argv(&["event", "creat", "x"])), None);
    }

    #[test]
    fn suggest_skips_option_values() {
        let names = ["event", "status", "config"];
        // `evnt` is `--home`'s value, not the command position.
        assert_eq!(
            suggest(&all(&names), &argv(&["--home", "evnt", "status"])),
            None
        );
        // an inline value carries its own `=`, so the next word is the command.
        assert_eq!(
            suggest(&all(&names), &argv(&["--home=evnt", "stauts"])),
            Some("status".to_string())
        );
    }

    #[test]
    fn suggest_stops_at_double_dash() {
        let names = ["event", "status", "config"];
        assert_eq!(suggest(&all(&names), &argv(&["--", "evnt"])), None);
    }

    #[test]
    fn suggest_ignores_exact_flags_and_distant_words() {
        let names = ["event", "status"];
        // exact matches need no suggestion
        assert_eq!(suggest(&all(&names), &argv(&["status"])), None);
        // flags are not commands
        assert_eq!(suggest(&all(&names), &argv(&["--home"])), None);
        // too far away (> 2 edits)
        assert_eq!(suggest(&all(&names), &argv(&["wildlydifferent"])), None);
        // nothing typed at all
        assert_eq!(suggest(&all(&names), &argv(&[])), None);
    }

    #[test]
    fn suggest_prefers_the_closest_name() {
        let names = ["config", "configure"];
        assert_eq!(
            suggest(&all(&names), &argv(&["confg"])),
            Some("config".to_string())
        );
    }

    #[test]
    fn suggest_breaks_ties_by_registration_order() {
        let names = ["aa", "ab"];
        assert_eq!(
            suggest(&all(&names), &argv(&["ac"])),
            Some("aa".to_string())
        );
    }

    #[test]
    fn closest_finds_a_name_one_or_two_edits_away() {
        let names = ["sync", "status", "cancel"];
        assert_eq!(closest("snyc", names), Some("sync"));
        assert_eq!(closest("stats", names), Some("status"));
        assert_eq!(closest("cancle", names), Some("cancel"));
    }

    #[test]
    fn closest_ignores_exact_and_distant_words() {
        let names = ["sync", "status"];
        assert_eq!(closest("sync", names), None);
        assert_eq!(closest("zzzzzz", names), None);
        assert_eq!(closest("snyc", []), None);
    }

    #[test]
    fn closest_prefers_the_nearest_then_the_earlier_name() {
        assert_eq!(closest("confg", ["configure", "config"]), Some("config"));
        assert_eq!(closest("ac", ["aa", "ab"]), Some("aa"));
        assert_eq!(closest("ac", ["ab", "aa"]), Some("ab"));
    }

    #[test]
    fn suggest_covers_registered_externals() {
        // externals push their name onto the same registry as leaves/groups.
        let names = ["event", "status", "foo"];
        assert_eq!(
            suggest(&all(&names), &argv(&["fooo"])),
            Some("foo".to_string())
        );
    }

    #[test]
    fn suggested_appends_a_line_when_bpaf_was_silent() {
        let out = suggested(
            ParseFailure::Stderr(bpaf::doc::Doc::from("no such command")),
            &all(&["status"]),
            &argv(&["stauts"]),
        );
        assert_eq!(
            out.unwrap_stderr(),
            "no such command\ndid you mean `status`?"
        );
    }

    #[test]
    fn suggested_passes_help_and_completion_through() {
        let r = all(&["status"]);
        let typo = argv(&["stauts"]);
        let help = suggested(
            ParseFailure::Stdout(bpaf::doc::Doc::from("Usage: stauts"), false),
            &r,
            &typo,
        );
        assert_eq!(help.unwrap_stdout(), "Usage: stauts");
        let completion = suggested(ParseFailure::Completion("stauts\n".into()), &r, &typo);
        assert_eq!(completion.unwrap_stdout(), "stauts\n");
    }

    #[test]
    fn report_outcome_returns_the_exit_code() {
        // That a `Success` is *silent* is structural — it has no message to
        // print; here only the returned code is asserted.
        assert_eq!(report_outcome(Ok(Success::OK)), 0);
        assert_eq!(report_outcome(Ok(Success::code(7))), 7);
        assert_eq!(report_outcome(Err(Failure::usage("u"))), 2);
        assert_eq!(report_outcome(Err(Failure::runtime("boom"))), 1);
    }

    /// bpaf's rule — every positional after every option — is only checked
    /// when help renders, so a debug build renders it once at fold time to
    /// fail where the tree is built rather than at a user's `--help`.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "positional and command items")]
    fn a_positional_before_an_option_panics_when_folded() {
        let p = bpaf::positional::<String>("P");
        let o = bpaf::long("opt").switch();
        let bad = crate::command("bad", "bad", construct!(p, o), |_, _: &()| {});
        let _ = App::new(bpaf::pure(())).command(bad).into_parser();
    }

    /// … however deep the command sits: here, a curried command two levels
    /// down, with its positional before its option.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "positional and command items")]
    fn a_nested_positional_before_an_option_panics_when_folded() {
        let p = bpaf::positional::<String>("P");
        let o = bpaf::long("opt").switch();
        let bad = crate::curried_command("bad", "bad", construct!(p, o), |_, _: String, _: &()| {});
        let source = crate::group_args(
            "source",
            "sources",
            || bpaf::positional::<String>("SOURCE"),
            vec![cmd("list")],
            vec![bad],
        );
        let outer = crate::group("outer", "outer", vec![source]);
        let _ = App::new(bpaf::pure(())).command(outer).into_parser();
    }

    #[test]
    fn program_name_is_the_file_name_of_argv0() {
        assert_eq!(program_name("/usr/bin/playr").as_deref(), Some("playr"));
        assert_eq!(program_name("playr").as_deref(), Some("playr"));
        assert_eq!(
            program_name("./target/debug/playr").as_deref(),
            Some("playr")
        );
        assert_eq!(program_name(""), None);
    }

    #[test]
    fn candidate_values_reads_a_lone_item() {
        // One candidate: printed without its description.
        assert_eq!(candidate_values("alice\n"), ["alice"]);
    }

    #[test]
    fn candidate_values_skips_the_metavariable_placeholder() {
        // A value completer's metavariable is an item with an empty value.
        assert_eq!(
            candidate_values("\tthe source\nbandcamp\nbandit\n"),
            ["bandcamp", "bandit"]
        );
    }

    #[test]
    fn candidate_values_keeps_the_first_of_several_commands() {
        // No header line: the first line is a candidate like any other.
        assert_eq!(
            candidate_values("sync\tsync a source\nstatus\tshow\n"),
            ["sync", "status"]
        );
    }

    #[test]
    fn candidate_values_of_nothing_applicable_is_empty() {
        let none: [&str; 0] = [];
        assert_eq!(candidate_values(""), none);
        assert_eq!(candidate_values("\n"), none);
    }

    #[test]
    fn suggested_keeps_bpafs_own_suggestion() {
        // Capital `D`: detection is case-insensitive.
        let msg = "No such command: `stauts`. Did you mean `status`?";
        let out = suggested(
            ParseFailure::Stderr(bpaf::doc::Doc::from(msg)),
            &all(&["status"]),
            &argv(&["stauts"]),
        );
        assert_eq!(out.unwrap_stderr(), msg);
    }
}
