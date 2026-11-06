//! Arguments are types: the [`Arg`] trait and the completion helpers.
//!
//! A command's parameter is one argument **type**, and this is where that type says
//! how it parses — and, optionally, what it completes to. [`Arg::parser`] is the
//! whole trait: one associated constructor, no metavar, no attributes, nothing
//! about spelling. The spelling is on the type, written one of two ways by
//! [`#[derive(Arg)]`](derive@crate::Arg) (ADR-016):
//!
//! - **spelled** — `#[arg(positional = "KEY")]`, `#[arg(long, short = 's',
//!   metavar = "FIELD")]`, … on a one-value type. hicli writes the `bpaf` chain,
//!   the type's doc comment's first paragraph is its help row, and the type
//!   also implements [`ArgHelp`], so a command's `# Arguments` entry can replace
//!   that row. No `#[derive(Bpaf)]`, no direct `bpaf` dependency;
//! - **bpaf mode** — no `positional`, `long` or `short` key. The spelling lives
//!   in `#[derive(Bpaf)]` on the same type, exactly as it did before spelled
//!   mode existed, and the derive calls its constructor. This is the form for a
//!   type with several fields.
//!
//! ```
//! use hicli::Arg;
//! # use hicli::bpaf::Parser;
//!
//! /// An event id
//! #[derive(Debug, Clone, Arg)]
//! #[arg(positional = "ID")]
//! pub struct Id(pub String);
//!
//! let id = <Id as Arg>::parser().to_options().run_inner(&["e-17"][..]).unwrap();
//! assert_eq!(id.0, "e-17");
//! ```
//!
//! # When a completer fires
//!
//! **Spelled mode completes the word.** A spelled type's `complete = f` is a
//! `fn(&str) -> Vec<Candidate>` handed the **raw word** as typed so far, before
//! it is parsed, so it fires on the empty word and on a half-typed one, even
//! for a `u16` — and a [`Choice`](trait@crate::Choice) inside completes its
//! names with no `complete` key at all.
//!
//! **bpaf mode completes what already parses.** A completer attached to a
//! `#[derive(Bpaf)]` type, or through [`complete_with`], is handed the value
//! that **already parsed** — `&T`, not the raw word. That one fact decides when
//! it fires:
//!
//! - a `String` (and a `Vec<String>` under `some(..)`, which is handed every
//!   word collected so far with the in-progress one last) parses the empty word,
//!   so the completer runs from the first keystroke;
//! - a `u64` — or anything validated through `.parse(..)` — does **not** parse
//!   the empty word, so the completer is never called for it.
//!
//! When that matters, spell the type with `#[arg(…)]` instead, or hand-write
//! [`Arg::parser`]: complete at the **string** level, and validate after —
//! which is what spelled mode writes for you.
//!
//! ```
//! use hicli::bpaf::{long, Parser};
//! use hicli::{complete_with, Arg};
//!
//! /// A TCP port.
//! pub struct Port(u16);
//!
//! impl Arg for Port {
//!     fn parser() -> impl Parser<Self> {
//!         // complete the *word*, which always parses …
//!         complete_with(long("port").argument::<String>("PORT"), |typed: &String| {
//!             ["80", "443", "8080"]
//!                 .iter()
//!                 .filter(|p| p.starts_with(typed.as_str()))
//!                 .map(|p| ((*p).to_string(), None))
//!                 .collect()
//!         })
//!         // … and validate after, where a half-typed port is allowed to fail.
//!         .parse(|s| s.parse::<u16>().map(Port))
//!     }
//! }
//!
//! let p = <Port as Arg>::parser()
//!     .to_options()
//!     .run_inner(&["--port", "8080"][..])
//!     .expect("parses");
//! assert_eq!(p.0, 8080);
//! ```
//!
//! # The completer filters
//!
//! `bpaf` passes a completer's output through **unfiltered**. It filters its own
//! flag names; it does not filter yours. A completer that ignores the word it is
//! handed therefore offers every candidate at every keystroke — the filtering is
//! the completer's job, which is why every example here starts with
//! `.filter(|c| c.starts_with(typed))`.
//!
//! # `hicli` ships no `Arg` impls
//!
//! Not for `String`, not for `u64`, not for `PathBuf` — on purpose. An argument
//! is a type: the metavar (`"TITLE"`), and whether it is a positional or an
//! option, are yours to name, and a blanket impl would have to invent both. Give
//! the value a documented newtype, and spell it:
//!
//! ```ignore
//! /// An event id
//! #[derive(Arg)]
//! #[arg(positional = "ID")]
//! struct Id(String);
//!
//! // or, in bpaf mode:
//! #[derive(Bpaf, Arg)]
//! struct Id(#[bpaf(positional("ID"))] String);
//! ```
//!
//! A `Globals` struct is **not** an `Arg` either, and must not implement it:
//! globals are composed at the top level by [`App::new`](crate::App::new), never
//! as a command's parameter, so they never go through [`Arg::parser`].
//!
//! # Composites
//!
//! A type made of argument types composes their parsers, and each part keeps its
//! own completion:
//!
//! ```
//! # use hicli::bpaf::{construct, positional, Parser};
//! # use hicli::Arg;
//! # struct Dir(String);
//! # impl Arg for Dir { fn parser() -> impl Parser<Self> { positional::<String>("DIR").map(Dir) } }
//! # struct Instance(String);
//! # impl Arg for Instance {
//! #     fn parser() -> impl Parser<Self> { positional::<String>("INSTANCE").map(Instance) }
//! # }
//! struct EventRef {
//!     dir: Dir,
//!     instance: Instance,
//! }
//!
//! impl Arg for EventRef {
//!     fn parser() -> impl Parser<Self> {
//!         let dir = <Dir as Arg>::parser();
//!         let instance = <Instance as Arg>::parser();
//!         construct!(EventRef { dir, instance })
//!     }
//! }
//! ```
//!
//! # A value that starts with `-`
//!
//! `bpaf` splits the command line into flags and words **before** any parser
//! runs, so in `--since -7` the `-7` is already a short flag by the time
//! `long("since").argument(..)` looks for its value. A value that starts with
//! `-` is therefore written glued to its option, `--since=-7`, and a plain
//! `argument` reads it — no special parser needed. The two-word form
//! `--since -7` is a usage error, and `bpaf`'s message says what to write
//! instead. This keeps the standard command-line rule that a word starting with
//! `-` is an option:
//!
//! ```
//! use hicli::bpaf::{long, Parser};
//! use hicli::Arg;
//!
//! /// A relative date, in days: `-7` is a week ago.
//! pub struct Since(String);
//!
//! impl Arg for Since {
//!     fn parser() -> impl Parser<Self> {
//!         long("since").argument::<String>("DAYS").map(Since)
//!     }
//! }
//!
//! let p = || <Since as Arg>::parser().to_options();
//! assert_eq!(p().run_inner(&["--since=-7"][..]).ok().expect("glued parses").0, "-7");
//! let e = p().run_inner(&["--since", "-7"][..]).err().expect("two words is a usage error");
//! assert_eq!(
//!     e.unwrap_stderr(),
//!     "`--since` requires an argument `DAYS`, got a flag `-7`, \
//!      try `--since=-7` to use it as an argument"
//! );
//! ```
//!
//! # What a doc comment on an argument type is
//!
//! It depends on the mode:
//!
//! - **spelled** — the doc comment is the type's **help row**: its first
//!   paragraph, its lines joined by a space. Everything after the first blank
//!   line is rustdoc only and never printed, so developer prose can live
//!   there. A missing or blank doc comment is a compile error. A doc comment
//!   at a *use site* — a field of a `#[derive(Bpaf)]` struct that embeds the
//!   type through `#[bpaf(external(…))]` — is never rendered; only a command's
//!   `# Arguments` entry overrides the row.
//! - **bpaf mode** — `bpaf` renders the `///` doc comment on a
//!   `#[derive(Bpaf)]` struct as a **group heading** above that struct's
//!   options in `--help`, so a struct documented `/// Filtering` lists its
//!   options under `Filtering`. Keep it to a single user-facing line (or leave
//!   it off), document the fields instead, and put developer prose in `//`
//!   comments beside the type.
//!
//! # Completion never changes the order
//!
//! Adding completion to a type does not change whether it is a positional:
//! [`complete_with`] / [`complete_files`] / [`complete_dirs`] wrap the parser
//! they are given and produce the same thing with a completer attached. The
//! ordering rule — every argument type containing a positional comes *after* the
//! ones that do not — is exactly what it was before.

use bpaf::{Parser, ShellComp};

/// One completion candidate: the value to insert, and an optional description
/// shown beside it.
///
/// The description is advisory — the wire protocol drops it when there is only
/// one candidate, because a shell that has exactly one answer inserts it rather
/// than showing a menu.
pub type Candidate = (String, Option<String>);

/// How an argument type parses: one constructor, and nothing else.
///
/// Implement it by hand, or derive it. `#[derive(Arg)]` with
/// `#[arg(positional|long|short …)]` spells a one-value type itself; without
/// those keys it goes beside `#[derive(Bpaf)]`, calls the type's snake_case
/// `bpaf` constructor (`Id` → `id()`) and optionally wraps it in one of the
/// completion helpers. See [`#[derive(Arg)]`](derive@crate::Arg).
///
/// ```
/// use hicli::bpaf::{positional, Parser};
/// use hicli::Arg;
///
/// /// The title of an event.
/// struct Title(String);
///
/// impl Arg for Title {
///     fn parser() -> impl Parser<Self> {
///         positional::<String>("TITLE").map(Title)
///     }
/// }
///
/// let t = <Title as Arg>::parser()
///     .to_options()
///     .run_inner(&["Summer Party"][..])
///     .expect("parses");
/// assert_eq!(t.0, "Summer Party");
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a hicli argument type",
    label = "this parameter's type must implement `Arg`",
    note = "an argument is a type: give it a documented newtype — `/// An event id` above `#[derive(Arg)] #[arg(positional = \"ID\")] struct Id(String);`",
    note = "a choice enum is spelled the same way: `#[derive(Choice, Arg)] #[arg(positional = \"KEY\")]`; a type with several fields is `#[derive(Bpaf, Arg)]`",
    note = "hicli ships no `Arg` impls for `String`, `u64`, `PathBuf`, … on purpose: the metavar and positional-vs-option are yours to name"
)]
pub trait Arg: Sized + 'static {
    /// The `bpaf` parser for this argument type.
    fn parser() -> impl Parser<Self>;
}

/// An argument type whose row help can be replaced where it is used.
///
/// A type spelled by `#[derive(Arg)]` with `#[arg(positional|long|short …)]`
/// builds its own help row, so it implements this trait: [`Arg::parser`] is
/// `parser_with_help` with the type's doc comment, and a command whose doc
/// comment has an `# Arguments` entry for the parameter calls
/// `parser_with_help` with that entry instead.
///
/// It is a trait of its own, not a defaulted method on [`Arg`], so that an
/// override on a type that cannot honour it — one spelled by
/// `#[derive(Bpaf)]`, whose help `bpaf` builds — is a compile error instead of
/// silently ignored. A hand-written [`Arg`] may implement it too:
///
/// ```
/// use hicli::bpaf::{positional, Parser};
/// use hicli::{Arg, ArgHelp};
///
/// struct Title(String);
///
/// impl ArgHelp for Title {
///     fn parser_with_help(help: &'static str) -> impl Parser<Self> {
///         positional::<String>("TITLE").help(help).map(Title)
///     }
/// }
///
/// impl Arg for Title {
///     fn parser() -> impl Parser<Self> {
///         <Title as ArgHelp>::parser_with_help("The event's title")
///     }
/// }
///
/// let t = <Title as ArgHelp>::parser_with_help("x")
///     .to_options()
///     .run_inner(&["Summer Party"][..])
///     .expect("parses");
/// assert_eq!(t.0, "Summer Party");
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}`'s help cannot be overridden",
    label = "named in this command's `# Arguments`",
    note = "only a type spelled by `#[arg(positional|long|short …)]` builds its own help; one spelled by `#[derive(Bpaf)]` keeps bpaf's",
    note = "spell the type with `#[arg(…)]`, or remove its entry from `# Arguments`"
)]
pub trait ArgHelp: Arg {
    /// This type's parser, with `help` as its row's help.
    fn parser_with_help(help: &'static str) -> impl Parser<Self>;
}

/// Attach a dynamic completer to `parser`.
///
/// `candidates` is handed the value that **already parsed** and returns what to
/// offer. `bpaf` does not filter the result, so filter it here.
///
/// ```
/// use hicli::bpaf::{positional, Parser};
/// use hicli::{complete_with, Candidate};
///
/// fn names(typed: &String) -> Vec<Candidate> {
///     ["alice", "alfred", "bo"]
///         .iter()
///         .filter(|n| n.starts_with(typed.as_str()))
///         .map(|n| ((*n).to_string(), Some("a person".to_string())))
///         .collect()
/// }
///
/// // Completion is additive: the parser still parses exactly what it did.
/// let who = complete_with(positional::<String>("WHO"), names)
///     .to_options()
///     .run_inner(&["alice"][..])
///     .expect("parses");
/// assert_eq!(who, "alice");
/// ```
///
/// A completer over a collected `Vec<String>` takes `&Vec<String>` — not
/// `&[String]` — because that is the parsed value's type, so its function needs
/// `#[allow(clippy::ptr_arg)]`.
pub fn complete_with<T>(
    parser: impl Parser<T>,
    candidates: impl Fn(&T) -> Vec<Candidate>,
) -> impl Parser<T> {
    parser.complete(candidates)
}

/// Complete `parser` with the shell's own **file** completion.
///
/// This is a directive to the shell, not a list of values: the shell does the
/// listing, which is why it knows about symlinks, mounts and quoting and
/// hicli does not. [`App::complete`](crate::App::complete) therefore reports
/// no candidates for it — there are none to report, only an instruction.
///
/// ```
/// use hicli::bpaf::{positional, Parser};
/// use hicli::complete_files;
///
/// let f = complete_files(positional::<String>("FILE"))
///     .to_options()
///     .run_inner(&["Cargo.toml"][..])
///     .expect("parses");
/// assert_eq!(f, "Cargo.toml");
/// ```
pub fn complete_files<T>(parser: impl Parser<T>) -> impl Parser<T> {
    parser.complete_shell(ShellComp::File { mask: None })
}

/// Complete `parser` with the shell's own **directory** completion.
///
/// The [`complete_files`] note applies unchanged: it is a directive to the
/// shell, so no candidate values come back through
/// [`App::complete`](crate::App::complete).
///
/// ```
/// use hicli::bpaf::{positional, Parser};
/// use hicli::complete_dirs;
///
/// let d = complete_dirs(positional::<String>("DIR"))
///     .to_options()
///     .run_inner(&["src"][..])
///     .expect("parses");
/// assert_eq!(d, "src");
/// ```
pub fn complete_dirs<T>(parser: impl Parser<T>) -> impl Parser<T> {
    parser.complete_shell(ShellComp::Dir { mask: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::completions;
    use bpaf::positional;

    /// The title of an event — a hand-written `Arg`, which is the form the
    /// derive has to agree with.
    struct Title(String);

    impl Arg for Title {
        fn parser() -> impl Parser<Self> {
            positional::<String>("TITLE").map(Title)
        }
    }

    #[test]
    fn a_hand_written_arg_parses() {
        let t = <Title as Arg>::parser()
            .to_options()
            .run_inner(&["Summer Party"][..])
            .expect("parses");
        assert_eq!(t.0, "Summer Party");
    }

    #[test]
    fn complete_with_offers_what_the_completer_returned() {
        // The completer filters — bpaf passes its output through — so `bo` is
        // absent because *this function* dropped it, not because bpaf did.
        let p = complete_with(positional::<String>("WHO"), |typed: &String| {
            ["alice", "alfred", "bo"]
                .iter()
                .filter(|n| n.starts_with(typed.as_str()))
                .map(|n| ((*n).to_string(), None))
                .collect()
        });
        let got = completions(p, "al");
        assert!(got.contains(&"alice".to_string()), "{got:?}");
        assert!(got.contains(&"alfred".to_string()), "{got:?}");
        assert!(!got.contains(&"bo".to_string()), "{got:?}");
    }

    #[test]
    fn a_shell_completion_still_parses_normally() {
        // A `ShellComp` is an instruction to the shell, not a value, so there is
        // nothing to assert about its candidates — only that attaching one left
        // the parser alone.
        let d = complete_dirs(positional::<String>("DIR"))
            .to_options()
            .run_inner(&["src"][..])
            .expect("parses");
        assert_eq!(d, "src");
    }
}
