//! The composition builder: [`App`] and exit-code handling.

use crate::command::{alternative, Command, IntoCommand, Mount};
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
}

impl<G: 'static, Ctx: 'static> App<G, Ctx> {
    /// Start an app from a global-options parser.
    pub fn new(globals: impl Parser<G> + 'static) -> Self {
        App {
            globals: globals.boxed(),
            commands: Vec::new(),
        }
    }

    /// Add one top-level command.
    ///
    /// Takes anything [`IntoCommand`]: a [`Command`] built with
    /// [`command`](crate::command) / [`group`](crate::group) /
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

    /// Fold the tree: the **top-level** command names (for the suggestion
    /// gap-closer) and the parser.
    ///
    /// The name registry is *derived* here rather than accumulated at
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
    /// Panics if no command was registered, or if a registered group is empty.
    fn fold(self) -> (Vec<&'static str>, OptionParser<(G, Invocation<Ctx>)>) {
        let App { globals, commands } = self;
        let names: Vec<&'static str> = commands.iter().map(Command::name).collect();
        let tree = alternative(commands, &Mount::root())
            .expect("hicli::App has no commands; add at least one via .command/.commands");
        (names, construct!(globals, tree).to_options())
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
    /// Equivalent to `ExitCode::from(run_with_args(std::env::args().skip(1),
    /// mk_ctx))`; use [`App::run_with_args`] in tests, since `ExitCode` is not
    /// comparable and a test seam must be assertable.
    ///
    /// # Panics
    ///
    /// Routes through [`App::into_parser`], so it panics if no command was
    /// registered.
    pub fn run(self, mk_ctx: impl FnOnce(G) -> Result<Ctx, Failure>) -> ExitCode {
        ExitCode::from(self.run_with_args(std::env::args().skip(1), mk_ctx))
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
    /// [`Failure`]'s message still go to the process's real stdout/stderr, which
    /// this method does not capture or redirect. Assert rendered text through
    /// [`App::parse`] (which returns the `ParseFailure` instead of printing it).
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
        let (names, parser) = self.fold();
        match parser.run_inner(refs.as_slice()) {
            Ok((globals, invocation)) => match mk_ctx(globals) {
                Ok(ctx) => report_outcome(invocation.run(&ctx)),
                Err(e) => report_outcome(Err(e)),
            },
            Err(failure) => report_failure(failure, &names, &args),
        }
    }
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
/// The wire format has two shapes, and the difference is the *count*:
///
/// - exactly one candidate — `"<value>\n"`, with no header and no description
///   (a shell with one answer inserts it rather than showing a menu);
/// - otherwise — a header line (`"<group>\t<help>\n"`, possibly just `"\n"`)
///   followed by one `"<value>\t<description>\n"` or `"<value>\n"` per
///   candidate.
///
/// Nothing applicable is the empty string, so a single line is never a lone
/// header: it is always the one-candidate shape.
pub(crate) fn candidate_values(out: &str) -> Vec<String> {
    let lines: Vec<&str> = out.lines().collect();
    let items: &[&str] = match lines.len() {
        0 | 1 => &lines,
        _ => &lines[1..],
    };
    items
        .iter()
        .map(|l| l.split('\t').next().unwrap_or(""))
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
fn report_failure(failure: ParseFailure, names: &[&str], args: &[String]) -> u8 {
    let code = exit_code_for(&failure);
    if matches!(failure, ParseFailure::Stderr(..)) {
        eprintln!("{}", augment_stderr(failure.unwrap_stderr(), names, args));
    } else {
        // Verified against bpaf 0.9.27: `unwrap_stdout()` does not panic on
        // `ParseFailure::Completion` — it returns the completion payload — so
        // this branch covers help/version *and* shell completion.
        print!("{}", failure.unwrap_stdout());
    }
    code
}

/// Print a command's outcome and return its process exit code.
///
/// A [`Failure`]'s message goes to stderr; a [`Success`] prints nothing at all,
/// not even a blank line. It is produced by a command that has already said
/// whatever there was to say (an external subcommand's child writes its own
/// stdout/stderr), so anything hicli added would be noise.
fn report_outcome(outcome: Result<Success, Failure>) -> u8 {
    match outcome {
        Ok(s) => s.get(),
        Err(e) => {
            eprintln!("{}", e.message());
            e.code()
        }
    }
}

/// The stderr text to print for a usage error: `msg`, plus a `did you mean`
/// line when `bpaf` produced no suggestion of its own.
fn augment_stderr(msg: String, names: &[&str], args: &[String]) -> String {
    let mut msg = msg;
    if !msg.to_lowercase().contains("did you mean") {
        if let Some(name) = suggest(names, args) {
            msg.push_str(&format!("\ndid you mean `{name}`?"));
        }
    }
    msg
}

/// The suggestion gap-closer: a Levenshtein pass (edit distance 1..=2) over the
/// registered top-level command names, applied to the **command position only**.
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
fn suggest(names: &[&str], args: &[String]) -> Option<String> {
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
        if names.contains(&tok.as_str()) {
            return None;
        }
        let mut best: Option<(usize, &str)> = None;
        for &name in names {
            let d = strsim::levenshtein(tok, name);
            if (1..=2).contains(&d) && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, name));
            }
        }
        return best.map(|(_, name)| name.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `["a", "b"]` as the `&[String]` `suggest` takes.
    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn suggest_matches_the_command_position() {
        let names = ["event", "status", "config"];
        assert_eq!(
            suggest(&names, &argv(&["stauts"])),
            Some("status".to_string())
        );
        assert_eq!(
            suggest(&names, &argv(&["evnt", "x"])),
            Some("event".to_string())
        );
    }

    #[test]
    fn suggest_ignores_tokens_after_a_recognised_command() {
        let names = ["event", "status", "config"];
        assert_eq!(suggest(&names, &argv(&["status", "evnt"])), None);
        assert_eq!(suggest(&names, &argv(&["event", "creat", "x"])), None);
    }

    #[test]
    fn suggest_skips_option_values() {
        let names = ["event", "status", "config"];
        // `evnt` is `--home`'s value, not the command position.
        assert_eq!(suggest(&names, &argv(&["--home", "evnt", "status"])), None);
        // an inline value carries its own `=`, so the next word is the command.
        assert_eq!(
            suggest(&names, &argv(&["--home=evnt", "stauts"])),
            Some("status".to_string())
        );
    }

    #[test]
    fn suggest_stops_at_double_dash() {
        let names = ["event", "status", "config"];
        assert_eq!(suggest(&names, &argv(&["--", "evnt"])), None);
    }

    #[test]
    fn suggest_ignores_exact_flags_and_distant_words() {
        let names = ["event", "status"];
        // exact matches need no suggestion
        assert_eq!(suggest(&names, &argv(&["status"])), None);
        // flags are not commands
        assert_eq!(suggest(&names, &argv(&["--home"])), None);
        // too far away (> 2 edits)
        assert_eq!(suggest(&names, &argv(&["wildlydifferent"])), None);
        // nothing typed at all
        assert_eq!(suggest(&names, &argv(&[])), None);
    }

    #[test]
    fn suggest_prefers_the_closest_name() {
        let names = ["config", "configure"];
        assert_eq!(
            suggest(&names, &argv(&["confg"])),
            Some("config".to_string())
        );
    }

    #[test]
    fn suggest_breaks_ties_by_registration_order() {
        let names = ["aa", "ab"];
        assert_eq!(suggest(&names, &argv(&["ac"])), Some("aa".to_string()));
    }

    #[test]
    fn suggest_covers_registered_externals() {
        // externals push their name onto the same registry as leaves/groups.
        let names = ["event", "status", "foo"];
        assert_eq!(suggest(&names, &argv(&["fooo"])), Some("foo".to_string()));
    }

    #[test]
    fn augment_stderr_appends_when_bpaf_was_silent() {
        let out = augment_stderr("no such command".into(), &["status"], &argv(&["stauts"]));
        assert!(
            out.ends_with("did you mean `status`?"),
            "expected an appended hint, got:\n{out}"
        );
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

    #[test]
    fn candidate_values_reads_the_one_candidate_shape() {
        // One candidate: no header line, and the description is already gone.
        assert_eq!(candidate_values("alice\n"), ["alice"]);
    }

    #[test]
    fn candidate_values_reads_the_header_shape() {
        // Two or more: a header line first, then one line each, with or without
        // a tab-separated description.
        assert_eq!(
            candidate_values("WHO\ta person\nalice\tthe boss\nalfred\n"),
            ["alice", "alfred"]
        );
        // The header may be blank — it is still a header.
        assert_eq!(candidate_values("\nalice\nalfred\n"), ["alice", "alfred"]);
    }

    #[test]
    fn candidate_values_of_nothing_applicable_is_empty() {
        let none: [&str; 0] = [];
        assert_eq!(candidate_values(""), none);
    }

    #[test]
    fn augment_stderr_keeps_an_existing_suggestion() {
        // Capital `D`: detection is case-insensitive.
        let msg = "No such command: `stauts`. Did you mean `status`?".to_string();
        assert_eq!(
            augment_stderr(msg.clone(), &["status"], &argv(&["stauts"])),
            msg
        );
    }
}
