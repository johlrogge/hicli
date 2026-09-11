//! The composition builder: [`App`], [`Group`], and exit-code handling.

use crate::external::ExternalVerb;
use crate::verb::{BoxedVerb, Exit};
use bpaf::{construct, OptionParser, ParseFailure, Parser};

/// A boxed alternative parser producing a [`BoxedVerb`].
type VerbParser<Ctx> = Box<dyn Parser<BoxedVerb<Ctx>>>;

/// Combine an accumulated alternative with one more branch.
///
/// The first branch that matches wins — this is what replaces the central
/// `match`: each `.leaf`/`.group`/`.external` folds one more command into the
/// alternative.
fn combine<Ctx: 'static>(acc: Option<VerbParser<Ctx>>, next: VerbParser<Ctx>) -> VerbParser<Ctx> {
    match acc {
        None => next,
        Some(acc) => construct!([acc, next]).boxed(),
    }
}

/// Wrap a leaf's parser as a named subcommand carrying `help`.
///
/// `help` is used twice on purpose: as the command's `descr` (the heading of
/// `app <cmd> --help`) and as its `help` (the one-line entry in the parent's
/// command list).
fn leaf_command<Ctx: 'static>(
    name: &'static str,
    help: &str,
    p: impl Parser<BoxedVerb<Ctx>> + 'static,
) -> impl Parser<BoxedVerb<Ctx>> {
    p.to_options().descr(help).command(name).help(help)
}

/// A nested set of subcommands, mounted under one name by
/// [`App::group`]/[`Group::group`].
///
/// Leaves and nested groups compose exactly like [`App`]; the whole group
/// becomes a single `command` in its parent.
pub struct Group<Ctx> {
    verbs: Option<VerbParser<Ctx>>,
}

impl<Ctx: 'static> Default for Group<Ctx> {
    fn default() -> Self {
        Group::new()
    }
}

impl<Ctx: 'static> Group<Ctx> {
    /// An empty group. Add commands with [`Group::leaf`]/[`Group::group`].
    pub fn new() -> Self {
        Group { verbs: None }
    }

    /// Add a leaf command named `name`, parsed by `p`.
    ///
    /// `help` is the one-line description shown both in the parent's command
    /// list and as the heading of `<cmd> --help`.
    pub fn leaf(
        mut self,
        name: &'static str,
        help: impl Into<String>,
        p: impl Parser<BoxedVerb<Ctx>> + 'static,
    ) -> Self {
        let help = help.into();
        let cmd = leaf_command(name, help.as_str(), p);
        self.verbs = Some(combine(self.verbs.take(), cmd.boxed()));
        self
    }

    /// Nest another group named `name`.
    ///
    /// `help` is the one-line description shown both in the parent's command
    /// list and as the heading of `<cmd> --help`.
    ///
    /// # Panics
    ///
    /// Panics if `sub` is empty — a group with no commands cannot be mounted.
    pub fn group(mut self, name: &'static str, help: impl Into<String>, sub: Group<Ctx>) -> Self {
        let help = help.into();
        let p = sub.into_command(name, help.as_str());
        self.verbs = Some(combine(self.verbs.take(), p.boxed()));
        self
    }

    /// Fold this group into a single subcommand parser mounted under `name`.
    fn into_command(self, name: &'static str, help: &str) -> impl Parser<BoxedVerb<Ctx>> {
        let alt = self
            .verbs
            .expect("verbandi::Group has no commands; add at least one via .leaf/.group");
        alt.to_options().descr(help).command(name).help(help)
    }
}

/// The composition builder.
///
/// `G` is the global-options type; `Ctx` is the shared context handed to every
/// verb. Build the tree with [`App::leaf`], [`App::group`] and
/// [`App::external`], then either [`App::run`] it or, in tests, drive it
/// through [`App::parse`] / [`App::into_parser`].
///
/// Note: `new` takes any `impl Parser<G>` (not `OptionParser<G>`); a plain
/// `#[derive(Bpaf)]` on the globals struct yields exactly such a parser, and
/// globals must compose *as a parser* with the command at the top level.
pub struct App<G, Ctx> {
    globals: Box<dyn Parser<G>>,
    verbs: Option<VerbParser<Ctx>>,
    /// Registry of **top-level** command names — read *only* by the suggestion
    /// gap-closer. Help and completion are rendered by `bpaf` from the
    /// `(name, help)` pair supplied at registration
    /// (`command(name).help(help)`), not from this list.
    ///
    /// Deliberately top-level only (see ADR
    /// "suggestion-registry-is-top-level-only"): nested command names live in
    /// their own subtree and are covered by `bpaf`'s own suggestions, which
    /// know which subtree the user is actually in.
    names: Vec<&'static str>,
}

impl<G: 'static, Ctx: 'static> App<G, Ctx> {
    /// Start an app from a global-options parser.
    pub fn new(globals: impl Parser<G> + 'static) -> Self {
        App {
            globals: globals.boxed(),
            verbs: None,
            names: Vec::new(),
        }
    }

    /// Add a top-level leaf command named `name`, parsed by `p`.
    ///
    /// `help` is the one-line description shown both in the parent's command
    /// list and as the heading of `<cmd> --help`.
    pub fn leaf(
        mut self,
        name: &'static str,
        help: impl Into<String>,
        p: impl Parser<BoxedVerb<Ctx>> + 'static,
    ) -> Self {
        self.names.push(name);
        let help = help.into();
        let cmd = leaf_command(name, help.as_str(), p);
        self.verbs = Some(combine(self.verbs.take(), cmd.boxed()));
        self
    }

    /// Mount a [`Group`] as the subcommand `name`.
    ///
    /// `help` is the one-line description shown both in the parent's command
    /// list and as the heading of `<cmd> --help`.
    ///
    /// # Panics
    ///
    /// Panics if `sub` is empty — a group with no commands cannot be mounted.
    pub fn group(mut self, name: &'static str, help: impl Into<String>, sub: Group<Ctx>) -> Self {
        self.names.push(name);
        let help = help.into();
        let p = sub.into_command(name, help.as_str());
        self.verbs = Some(combine(self.verbs.take(), p.boxed()));
        self
    }

    /// Register a git-style [`ExternalVerb`].
    pub fn external(mut self, ext: ExternalVerb) -> Self {
        self.names.push(ext.name());
        let p = ext.into_parser::<Ctx>();
        self.verbs = Some(combine(self.verbs.take(), p.boxed()));
        self
    }

    /// The top-level `bpaf` parser yielding `(globals, selected verb)`.
    ///
    /// This is a test seam: it lets a test render help/usage and inspect the
    /// selected verb without any process side effects.
    ///
    /// # Panics
    ///
    /// Panics if no command was registered — an app must have at least one
    /// [`leaf`](App::leaf)/[`group`](App::group)/[`external`](App::external).
    pub fn into_parser(self) -> OptionParser<(G, BoxedVerb<Ctx>)> {
        let verbs = self
            .verbs
            .expect("verbandi::App has no commands; add at least one via .leaf/.group/.external");
        let globals = self.globals;
        construct!(globals, verbs).to_options()
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
    pub fn parse<I, S>(self, args: I) -> Result<(G, BoxedVerb<Ctx>), ParseFailure>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let owned: Vec<String> = args.into_iter().map(Into::into).collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        self.into_parser().run_inner(refs.as_slice())
    }

    /// Run the app against `std::env::args()`.
    ///
    /// Steps: parse (remapping `bpaf`'s exit codes — usage `2`, help/version
    /// `0`, appending a `did you mean` suggestion when `bpaf` produced none),
    /// build the context via `mk_ctx`, then run the selected verb (mapping its
    /// [`Exit`] to `2`/`1`). Returns the process exit code.
    ///
    /// # Panics
    ///
    /// Routes through [`App::into_parser`], so it panics if no command was
    /// registered.
    pub fn run(self, mk_ctx: impl FnOnce(G) -> Result<Ctx, Exit>) -> i32 {
        let names = self.names.clone();
        let args: Vec<String> = std::env::args().skip(1).collect();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let parser = self.into_parser();
        match parser.run_inner(refs.as_slice()) {
            Ok((globals, verb)) => match mk_ctx(globals) {
                Ok(ctx) => match verb.run(&ctx) {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("{}", e.message());
                        e.code()
                    }
                },
                Err(e) => {
                    eprintln!("{}", e.message());
                    e.code()
                }
            },
            Err(failure) => report_failure(failure, &names, &args),
        }
    }
}

/// The remapped process exit code for a `bpaf` parse failure.
///
/// `bpaf` uses `1` for usage errors and `0` for help/version/completion; this
/// remaps usage errors to `2` (leaving help/version/completion at `0`).
pub fn exit_code_for(f: &ParseFailure) -> i32 {
    if matches!(f, ParseFailure::Stderr(..)) {
        2
    } else {
        0
    }
}

/// Print a parse failure to the right stream, appending a suggestion when
/// `bpaf` produced none, and return the remapped exit code.
fn report_failure(failure: ParseFailure, names: &[&str], args: &[String]) -> i32 {
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
/// Because verbandi does not know which globals take a value, a bare word
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
    fn augment_stderr_keeps_an_existing_suggestion() {
        // Capital `D`: detection is case-insensitive.
        let msg = "No such command: `stauts`. Did you mean `status`?".to_string();
        assert_eq!(
            augment_stderr(msg.clone(), &["status"], &argv(&["stauts"])),
            msg
        );
    }
}
