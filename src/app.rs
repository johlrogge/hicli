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
    pub fn leaf(mut self, name: &'static str, p: impl Parser<BoxedVerb<Ctx>> + 'static) -> Self {
        let cmd = p.to_options().command(name);
        self.verbs = Some(combine(self.verbs.take(), cmd.boxed()));
        self
    }

    /// Nest another group named `name`.
    pub fn group(mut self, name: &'static str, sub: Group<Ctx>) -> Self {
        let p = sub.into_command(name);
        self.verbs = Some(combine(self.verbs.take(), p.boxed()));
        self
    }

    /// Fold this group into a single subcommand parser mounted under `name`.
    fn into_command(self, name: &'static str) -> impl Parser<BoxedVerb<Ctx>> {
        let alt = self
            .verbs
            .expect("verbandi::Group has no commands; add at least one via .leaf/.group");
        alt.to_options().command(name)
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
    /// Registry of top-level command names — one source shared by help,
    /// completion and the suggestion gap-closer.
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
    pub fn leaf(mut self, name: &'static str, p: impl Parser<BoxedVerb<Ctx>> + 'static) -> Self {
        self.names.push(name);
        let cmd = p.to_options().command(name);
        self.verbs = Some(combine(self.verbs.take(), cmd.boxed()));
        self
    }

    /// Mount a [`Group`] as the subcommand `name`.
    pub fn group(mut self, name: &'static str, sub: Group<Ctx>) -> Self {
        self.names.push(name);
        let p = sub.into_command(name);
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
        let mut msg = failure.unwrap_stderr();
        if !msg.to_lowercase().contains("did you mean") {
            if let Some(name) = suggest(names, args) {
                msg.push_str(&format!("\ndid you mean `{name}`?"));
            }
        }
        eprintln!("{msg}");
    } else {
        print!("{}", failure.unwrap_stdout());
    }
    code
}

/// The suggestion gap-closer: a Levenshtein pass (edit distance 1..=2) over the
/// registered command names for the first non-flag token that isn't an exact
/// match. Shares [`App::names`] with help and completion.
fn suggest(names: &[&str], args: &[String]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for tok in args {
        if tok.starts_with('-') {
            continue;
        }
        if names.contains(&tok.as_str()) {
            continue;
        }
        for &name in names {
            let d = strsim::levenshtein(tok, name);
            if (1..=2).contains(&d) && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, name));
            }
        }
    }
    best.map(|(_, name)| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggest_matches_within_two_edits() {
        let names = ["event", "status", "config"];
        assert_eq!(
            suggest(&names, &["stauts".to_string()]),
            Some("status".to_string())
        );
        assert_eq!(
            suggest(&names, &["evnt".to_string()]),
            Some("event".to_string())
        );
    }

    #[test]
    fn suggest_skips_flags_exact_and_distant() {
        let names = ["event", "status"];
        // flags are not commands
        assert_eq!(suggest(&names, &["--home".to_string()]), None);
        // exact matches need no suggestion
        assert_eq!(suggest(&names, &["status".to_string()]), None);
        // too far away (> 2 edits)
        assert_eq!(suggest(&names, &["wildlydifferent".to_string()]), None);
    }
}
