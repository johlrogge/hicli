//! A value that is one of a fixed set of words: the [`Choice`] trait, its
//! error [`UnknownChoice`], and its completer [`complete_choice`].
//!
//! The module is private; its items are re-exported at the crate root, and the
//! consumer-facing story is on [`Choice`], where rustdoc shows it. The derive
//! is `hicli_macros::Choice`, re-exported beside the trait under the same name.
//!
//! A choice is a value. It is also an argument when `#[derive(Arg)]` spells it
//! (`#[derive(Choice, Arg)] #[arg(positional = "KEY")]`); the chain that
//! expansion ends in is `crate::private::choice`, which ends the help row with
//! the names (`What to sort by [bpm, title, artist]`, each name bold in
//! colour), completes with [`complete_choice`], parses with the `FromStr` the
//! derive wrote, and prefixes an error with the argument's name
//! (``couldn't parse `bmp`: `KEY`: expected `bpm`, `title` or `artist`; did you
//! mean `bpm`?``). A spelled newtype over a choice, or an `Option`/`Vec` of
//! one, gets the same help row through `crate::private::Probe` (ADR-018).

use std::fmt;
use std::str::FromStr;

use crate::arg::Candidate;

/// A value that is one of a fixed set of words.
///
/// A choice is an enum whose variants have no fields — `bpm`, `title`,
/// `by-artist` — written with [`#[derive(Choice)]`](derive@crate::Choice). The
/// derive gives the enum three things, all read off the variants, so a new
/// variant is a new word everywhere at once:
///
/// - [`FromStr`], matching each name exactly (case-sensitive), with an
///   [`UnknownChoice`] for anything else;
/// - [`Display`](fmt::Display), printing the name;
/// - this trait: [`VARIANTS`](Choice::VARIANTS), [`name`](Choice::name) and
///   [`description`](Choice::description) (the first paragraph of the
///   variant's doc comment, joined into one line).
///
/// A hand-written impl must agree with them: `FromStr` accepts exactly the
/// names of `VARIANTS`, and `Display` prints the name.
///
/// # Where a choice goes on the command line
///
/// A choice says what the words are, not where they go on the command line,
/// so the metavar and positional-vs-option are still yours to name.
/// [`#[derive(Arg)]`](derive@crate::Arg) in spelled mode names them, and a
/// choice it spells completes its names with no `complete` key — either the
/// enum itself is the argument, or a spelled newtype holds it:
///
/// ```
/// use hicli::{Arg, Choice};
/// # use hicli::bpaf::Parser;
///
/// /// What to sort by
/// #[derive(Debug, Clone, Copy, PartialEq, Choice, Arg)]
/// #[arg(positional = "KEY")]
/// pub enum SortKey {
///     /// Beats per minute
///     Bpm,
///     /// Track title
///     Title,
/// }
///
/// /// Sort the listing by this field
/// #[derive(Debug, Clone, Arg)]
/// #[arg(long = "sort", short = 's', metavar = "FIELD")]
/// pub struct SortOpt(pub Option<SortKey>);
///
/// let k = <SortKey as Arg>::parser().to_options().run_inner(&["title"][..]).unwrap();
/// assert_eq!(k, SortKey::Title);
/// let s = <SortOpt as Arg>::parser().to_options().run_inner(&[] as &[&str]).unwrap();
/// assert_eq!(s.0, None);
/// ```
///
/// Without `#[derive(Arg)]` a choice is only a value, and a command cannot
/// take it as a parameter. By hand, complete at the **string** level, so the
/// completer fires on the empty word and on a half-typed one, and parse after —
/// the core of the chain spelled mode writes, which also lists the names in
/// the help row and names the argument in the error:
///
/// ```
/// use hicli::bpaf::{positional, Parser};
/// use hicli::{complete_choice, Arg, Choice};
///
/// /// What to sort by.
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
/// pub enum SortKey {
///     /// Beats per minute
///     Bpm,
///     /// The track's title
///     Title,
///     #[choice(name = "by-artist")]
///     Artist,
/// }
///
/// /// The key a listing is sorted by.
/// pub struct Sort(SortKey);
///
/// impl Arg for Sort {
///     fn parser() -> impl Parser<Self> {
///         positional::<String>("KEY")
///             .complete(|typed: &String| complete_choice::<SortKey>(typed))
///             .parse(|typed: String| typed.parse::<SortKey>())
///             .map(Sort)
///     }
/// }
///
/// let s = <Sort as Arg>::parser()
///     .to_options()
///     .run_inner(&["by-artist"][..])
///     .expect("parses");
/// assert_eq!(s.0, SortKey::Artist);
///
/// let e = <Sort as Arg>::parser()
///     .to_options()
///     .run_inner(&["bmp"][..])
///     .err()
///     .expect("not a sort key");
/// assert_eq!(
///     e.unwrap_stderr(),
///     "couldn't parse `bmp`: expected `bpm`, `title` or `by-artist`; did you mean `bpm`?"
/// );
/// ```
///
/// # The error names the choices
///
/// [`UnknownChoice`] lists every name and, when one is close — the same word
/// in another case, or one edit away (two from six letters; a swapped pair of
/// letters is one edit) — suggests it. It does **not**
/// repeat the word that was typed: `bpaf` already prints
/// ``couldn't parse `bmp`: `` in front of it.
///
/// # A hand-written `Display` or `FromStr` is a conflict
///
/// The derive writes both, so an enum that also implements either by hand
/// fails with rustc's E0119, *conflicting implementations*. Write this impl by
/// hand instead, and call [`UnknownChoice::new`] from your `FromStr`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a hicli choice",
    label = "`{Self}` must implement `Choice`",
    note = "an enum spelled by `#[arg(...)]` is a choice: add `#[derive(Choice)]` to it"
)]
pub trait Choice: FromStr<Err = UnknownChoice> + fmt::Display + Sized + 'static {
    /// Every variant, in declaration order.
    const VARIANTS: &'static [Self];

    /// What the user types: the variant's ident in kebab-case, or its
    /// `#[choice(name = "…")]`.
    fn name(&self) -> &'static str;

    /// The first paragraph of the variant's doc comment, offered beside it
    /// when completing.
    fn description(&self) -> Option<&'static str>;
}

/// The error from parsing a [`Choice`] out of a word that is none of its
/// names.
///
/// It displays as the names it expected, and a suggestion when one is close:
///
/// ```text
/// expected `bpm`, `title` or `by-artist`; did you mean `bpm`?
/// ```
///
/// The typed word is not part of the message, because `bpaf` prints
/// ``couldn't parse `bmp`: `` in front of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownChoice {
    /// The word that was not a name.
    typed: String,
    /// Every name, in [`Choice::VARIANTS`] order.
    names: Vec<&'static str>,
    /// The name to suggest, if one is close.
    near: Option<&'static str>,
}

impl UnknownChoice {
    /// The error for `typed`, which is none of `C`'s names.
    ///
    /// The suggestion is the first name equal to `typed` ignoring ASCII case;
    /// failing that, the nearest name one edit away — two for a word of six
    /// letters or more — counting a swapped pair of letters as one edit.
    ///
    /// ```
    /// use hicli::{Choice, UnknownChoice};
    ///
    /// #[derive(Debug, Choice)]
    /// enum Mode { Fast, Slow }
    ///
    /// assert_eq!(
    ///     UnknownChoice::new::<Mode>("FAST").to_string(),
    ///     "expected `fast` or `slow`; did you mean `fast`?"
    /// );
    /// ```
    pub fn new<C: Choice>(typed: &str) -> Self {
        let names: Vec<&'static str> = C::VARIANTS.iter().map(Choice::name).collect();
        let near = names
            .iter()
            .copied()
            .find(|n| n.eq_ignore_ascii_case(typed))
            .or_else(|| crate::app::closest(typed, names.iter().copied()));
        UnknownChoice {
            typed: typed.to_string(),
            names,
            near,
        }
    }

    /// The word that was none of the names.
    pub fn typed(&self) -> &str {
        &self.typed
    }
}

impl fmt::Display for UnknownChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match crate::command::either(&self.names) {
            Some(list) => write!(f, "expected {list}")?,
            None => f.write_str("there is nothing to choose from")?,
        }
        if let Some(near) = self.near {
            write!(f, "; did you mean `{near}`?")?;
        }
        Ok(())
    }
}

impl std::error::Error for UnknownChoice {}

/// The names of `C` that start with `typed`, in [`Choice::VARIANTS`] order,
/// each with its [`description`](Choice::description).
///
/// A completer for a string-level parser, which is handed the word as typed so
/// far — the empty word included — and must filter, because `bpaf` does not:
///
/// ```
/// use hicli::{complete_choice, Choice};
///
/// #[derive(Debug, Choice)]
/// enum SortKey {
///     /// Beats per minute
///     Bpm,
///     Title,
/// }
///
/// assert_eq!(
///     complete_choice::<SortKey>("b"),
///     [("bpm".to_string(), Some("Beats per minute".to_string()))]
/// );
/// assert_eq!(complete_choice::<SortKey>("").len(), 2);
/// ```
pub fn complete_choice<C: Choice>(typed: &str) -> Vec<Candidate> {
    C::VARIANTS
        .iter()
        .filter(|c| c.name().starts_with(typed))
        .map(|c| (c.name().to_string(), c.description().map(str::to_string)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::completions;
    use crate::{cli, command, App, Arg};
    use bpaf::{long, positional, pure, Parser};

    /// A choice written by hand, the way the derive writes it — so these tests
    /// pin the runtime half without the macro.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SortKey {
        Bpm,
        Title,
        Artist,
    }

    impl Choice for SortKey {
        const VARIANTS: &'static [Self] = &[SortKey::Bpm, SortKey::Title, SortKey::Artist];

        fn name(&self) -> &'static str {
            match self {
                SortKey::Bpm => "bpm",
                SortKey::Title => "title",
                SortKey::Artist => "by-artist",
            }
        }

        fn description(&self) -> Option<&'static str> {
            match self {
                SortKey::Bpm => Some("Beats per minute"),
                SortKey::Title | SortKey::Artist => None,
            }
        }
    }

    impl FromStr for SortKey {
        type Err = UnknownChoice;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "bpm" => Ok(SortKey::Bpm),
                "title" => Ok(SortKey::Title),
                "by-artist" => Ok(SortKey::Artist),
                _ => Err(UnknownChoice::new::<Self>(s)),
            }
        }
    }

    impl fmt::Display for SortKey {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.name())
        }
    }

    /// A one-word choice.
    #[derive(Debug)]
    enum One {
        Only,
    }

    impl Choice for One {
        const VARIANTS: &'static [Self] = &[One::Only];
        fn name(&self) -> &'static str {
            "only"
        }
        fn description(&self) -> Option<&'static str> {
            None
        }
    }

    impl FromStr for One {
        type Err = UnknownChoice;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "only" => Ok(One::Only),
                _ => Err(UnknownChoice::new::<Self>(s)),
            }
        }
    }

    impl fmt::Display for One {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.name())
        }
    }

    /// A two-word choice.
    #[derive(Debug)]
    enum Two {
        Fast,
        Slow,
    }

    impl Choice for Two {
        const VARIANTS: &'static [Self] = &[Two::Fast, Two::Slow];
        fn name(&self) -> &'static str {
            match self {
                Two::Fast => "fast",
                Two::Slow => "slow",
            }
        }
        fn description(&self) -> Option<&'static str> {
            None
        }
    }

    impl FromStr for Two {
        type Err = UnknownChoice;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "fast" => Ok(Two::Fast),
                "slow" => Ok(Two::Slow),
                _ => Err(UnknownChoice::new::<Self>(s)),
            }
        }
    }

    impl fmt::Display for Two {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.name())
        }
    }

    /// The text `typed` fails to parse as `C` with.
    fn error<C: Choice + fmt::Debug>(typed: &str) -> String {
        typed
            .parse::<C>()
            .expect_err("not one of the names")
            .to_string()
    }

    #[test]
    fn one_name_is_expected_alone() {
        assert_eq!(error::<One>("xyzzy"), "expected `only`");
        assert_eq!(
            error::<One>("onyl"),
            "expected `only`; did you mean `only`?"
        );
    }

    #[test]
    fn two_names_are_joined_by_or() {
        assert_eq!(error::<Two>("medium"), "expected `fast` or `slow`");
        assert_eq!(
            error::<Two>("slwo"),
            "expected `fast` or `slow`; did you mean `slow`?"
        );
    }

    #[test]
    fn three_names_are_a_list_ending_in_or() {
        assert_eq!(
            error::<SortKey>("genre"),
            "expected `bpm`, `title` or `by-artist`"
        );
        assert_eq!(
            error::<SortKey>("bmp"),
            "expected `bpm`, `title` or `by-artist`; did you mean `bpm`?"
        );
    }

    #[test]
    fn the_suggestion_ignores_case_and_matching_does_not() {
        // `BPM` is three edits from `bpm`, too far for the edit distance: the
        // case-insensitive pass is what finds it.
        assert_eq!(
            error::<SortKey>("BPM"),
            "expected `bpm`, `title` or `by-artist`; did you mean `bpm`?"
        );
        assert_eq!(
            error::<SortKey>("By-Artist"),
            "expected `bpm`, `title` or `by-artist`; did you mean `by-artist`?"
        );
    }

    #[test]
    fn a_distant_word_gets_no_suggestion() {
        assert_eq!(
            error::<SortKey>("popularity"),
            "expected `bpm`, `title` or `by-artist`"
        );
    }

    #[test]
    fn a_short_word_allows_one_edit_only() {
        // `tcsh` is two edits from `bash`: too many for four letters.
        assert_eq!(
            error::<crate::Shell>("tcsh"),
            "expected `bash`, `zsh` or `fish`"
        );
        assert_eq!(error::<Two>("flsh"), "expected `fast` or `slow`");
        assert_eq!(
            error::<Two>("fas"),
            "expected `fast` or `slow`; did you mean `fast`?"
        );
    }

    #[test]
    fn a_six_letter_word_allows_two_edits() {
        assert_eq!(
            error::<SortKey>("titlexx"),
            "expected `bpm`, `title` or `by-artist`; did you mean `title`?"
        );
        assert_eq!(
            error::<SortKey>("byartst"),
            "expected `bpm`, `title` or `by-artist`; did you mean `by-artist`?"
        );
    }

    #[test]
    fn the_error_keeps_the_typed_word() {
        let e = "bmp".parse::<SortKey>().expect_err("not a sort key");
        assert_eq!(e.typed(), "bmp");
        assert_eq!(e, UnknownChoice::new::<SortKey>("bmp"));
        assert_ne!(e, UnknownChoice::new::<SortKey>("bpn"));
    }

    #[test]
    fn complete_choice_filters_by_prefix_with_descriptions() {
        assert_eq!(
            complete_choice::<SortKey>("b"),
            [
                ("bpm".to_string(), Some("Beats per minute".to_string())),
                ("by-artist".to_string(), None),
            ]
        );
        assert_eq!(
            complete_choice::<SortKey>("t"),
            [("title".to_string(), None)]
        );
        assert!(complete_choice::<SortKey>("x").is_empty());
        // The empty word is a prefix of every name: all of them, in order.
        let all: Vec<String> = complete_choice::<SortKey>("")
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(all, ["bpm", "title", "by-artist"]);
    }

    /// The string-level chain, positional: complete the word, parse after.
    fn sort_positional() -> impl Parser<SortKey> {
        positional::<String>("KEY")
            .complete(|w: &String| complete_choice::<SortKey>(w))
            .parse(|w: String| w.parse::<SortKey>())
    }

    /// The same chain, as an option's value.
    fn sort_option() -> impl Parser<SortKey> {
        long("sort")
            .argument::<String>("KEY")
            .complete(|w: &String| complete_choice::<SortKey>(w))
            .parse(|w: String| w.parse::<SortKey>())
    }

    #[test]
    fn a_positional_choice_completes_through_bpaf() {
        assert_eq!(completions(sort_positional(), "b"), ["bpm", "by-artist"]);
        assert_eq!(
            completions(sort_positional(), ""),
            ["bpm", "title", "by-artist"]
        );
        assert_eq!(completions(sort_positional(), "ti"), ["title"]);
    }

    #[test]
    fn an_option_choice_completes_through_bpaf() {
        let p = || sort_option().to_options();
        let out = |words: &[&str]| {
            let mut argv = vec![crate::app::COMPLETE_REV_1];
            argv.extend_from_slice(words);
            crate::app::candidate_values(
                &p().run_inner(argv.as_slice())
                    .expect_err("completes")
                    .unwrap_stdout(),
            )
        };
        assert_eq!(out(&["--sort", "b"]), ["bpm", "by-artist"]);
        assert_eq!(out(&["--sort", ""]), ["bpm", "title", "by-artist"]);
    }

    #[test]
    fn the_chain_parses_and_reports_through_bpaf() {
        let got = sort_positional()
            .to_options()
            .run_inner(&["by-artist"][..])
            .expect("parses");
        assert_eq!(got, SortKey::Artist);
        let e = sort_positional()
            .to_options()
            .run_inner(&["bmp"][..])
            .expect_err("not a sort key")
            .unwrap_stderr();
        assert_eq!(
            e,
            "couldn't parse `bmp`: expected `bpm`, `title` or `by-artist`; did you mean `bpm`?"
        );
    }

    /// The key a listing is sorted by — a hand-written `Arg` over the chain.
    struct Sort(SortKey);

    impl Arg for Sort {
        fn parser() -> impl Parser<Self> {
            sort_positional().map(Sort)
        }
    }

    /// An app whose `sort` command takes a `Sort`.
    fn app() -> App<(), ()> {
        App::new(pure(())).command(command(
            "sort",
            "sort the listing",
            <Sort as Arg>::parser(),
            |s: Sort, _: &()| {
                let _ = s.0;
            },
        ))
    }

    #[test]
    fn a_choice_completes_inside_a_hicli_command() {
        // The whole design rests on this: a completer attached at the string
        // level still fires once the parser is mounted under a command.
        assert_eq!(app().complete(cli! { sort b }), ["bpm", "by-artist"]);
        assert_eq!(app().complete(cli! { sort bp }), ["bpm"]);
        assert_eq!(
            app().complete(cli! { sort "" }),
            ["bpm", "title", "by-artist"]
        );
    }

    #[test]
    fn a_choice_parses_and_reports_inside_a_hicli_command() {
        let (_, inv) = app().parse(cli! { sort title }).expect("parses");
        assert_eq!(inv.path(), ["sort"]);
        let e = app()
            .parse(cli! { sort bmp })
            .err()
            .expect("not a sort key")
            .unwrap_stderr();
        assert_eq!(
            e,
            "couldn't parse `bmp`: expected `bpm`, `title` or `by-artist`; did you mean `bpm`?"
        );
    }
}
