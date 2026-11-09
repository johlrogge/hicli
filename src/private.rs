//! What `#[derive(Arg)]`'s spelled mode expands into, and nothing else.
//!
//! `#[doc(hidden)]` and not part of the API: the paths here are named by
//! generated code only, and may change in any release.
//!
//! # The completer probe
//!
//! A spelled option over a [`Choice`] completes its names on its own, without
//! a `complete = …` key. The derive cannot ask whether a type is a `Choice` —
//! a proc macro sees tokens, not traits — so it asks the compiler, through
//! method resolution ("autoref specialisation"):
//!
//! ```ignore
//! use ::hicli::__private::{IsChoice as _, NotChoice as _};
//! let complete = (&::hicli::__private::Probe::<SortField>::NEW).completer();
//! let help = (&::hicli::__private::Probe::<SortField>::NEW).help_row(help);
//! ```
//!
//! The receiver is `&Probe<T>`. Method lookup tries the receiver's own type
//! first, where only [`IsChoice`] has a method (its `&self` *is*
//! `&Probe<T>`), and that impl applies only when `T: Choice`. Failing that,
//! lookup auto-references to `&&Probe<T>`, where [`NotChoice`]'s impl for
//! `&Probe<T>` always applies. The type must be concrete at the call, which it
//! is in every expansion. The same probe writes the help row: a choice's
//! names follow its help (ADR-018).
//!
//! # The parse probe
//!
//! A spelled one-value type `T(I)` parses a word through `T`'s own `FromStr`
//! when it has one, and through `I`'s — then wraps — when it does not
//! (ADR-018). The same autoref trick asks which:
//!
//! ```ignore
//! use ::hicli::__private::{SelfFromStr as _, FieldFromStr as _};
//! let v = (&::hicli::__private::FromStrOf::<LimitArg, NonZeroUsize>(LimitArg))
//!     .parse_word(w.as_str());
//! ```
//!
//! The receiver is `&FromStrOf<T, I>`. [`SelfFromStr`] is implemented for
//! `FromStrOf<T, I>` when `T: FromStr`, so its `&self` matches the receiver
//! as it is; failing that, lookup auto-references and finds [`FieldFromStr`],
//! implemented for `&FromStrOf<T, I>` when `I: FromStr`. Neither impl bounds
//! the `Err`, so a type whose `FromStr` exists is never skipped over in
//! silence; when neither applies the call is E0599, naming both bounds.
//! [`SelfFromOs`] / [`FieldFromOs`] are the same pair for a path value read
//! by bpaf as a `PathBuf` / `OsString`.

use std::ffi::OsStr;
use std::fmt::{self, Display};
use std::marker::PhantomData;
use std::str::FromStr;

use bpaf::doc::Doc;
use bpaf::Parser;

use crate::arg::Candidate;
use crate::choice::{complete_choice, Choice};

/// A value that only carries a type, for method resolution to look at.
pub struct Probe<T>(PhantomData<T>);

impl<T> Probe<T> {
    /// The probe for `T`.
    pub const NEW: Self = Probe(PhantomData);
}

/// What a probed type that is a [`Choice`] brings: its names, as completions
/// and at the end of its help row.
pub trait IsChoice {
    /// The completer for the probed type.
    fn completer(&self) -> fn(&str) -> Vec<Candidate>;
    /// The help row for the probed type: `help`, then its names.
    fn help_row(&self, help: &str) -> Doc;
}

impl<T: Choice> IsChoice for Probe<T> {
    fn completer(&self) -> fn(&str) -> Vec<Candidate> {
        complete_choice::<T>
    }
    fn help_row(&self, help: &str) -> Doc {
        choice_help::<T>(help)
    }
}

/// What every other probed type brings: nothing.
pub trait NotChoice {
    /// The completer for the probed type.
    fn completer(&self) -> fn(&str) -> Vec<Candidate>;
    /// The help row for the probed type: `help`, unchanged.
    fn help_row(&self, help: &str) -> Doc;
}

impl<T> NotChoice for &Probe<T> {
    fn completer(&self) -> fn(&str) -> Vec<Candidate> {
        no_candidates
    }
    fn help_row(&self, help: &str) -> Doc {
        Doc::from(help)
    }
}

/// The completer that offers nothing.
pub fn no_candidates(_: &str) -> Vec<Candidate> {
    Vec::new()
}

/// A choice's help row: `help`, then ` [a, b, c]` with each name a literal
/// (bold in colour); nothing is appended when `C` has no variants. A help
/// ending in a single full stop gets the list before it: `… precedence
/// [never].` — an ellipsis or other punctuation stays before the list.
pub fn choice_help<C: Choice>(help: &str) -> Doc {
    if C::VARIANTS.is_empty() {
        return Doc::from(help);
    }
    let (body, stop) = match help.strip_suffix('.') {
        Some(body) if !body.ends_with('.') => (body, "."),
        _ => (help, ""),
    };
    let mut doc = Doc::from(body);
    doc.text(" [");
    for (i, c) in C::VARIANTS.iter().enumerate() {
        if i > 0 {
            doc.text(", ");
        }
        doc.literal(c.name());
    }
    doc.text("]");
    doc.text(stop);
    doc
}

/// A self-spelled choice enum's whole chain: the head built around its help
/// row, completion with `C`'s names, and a parse whose error is
/// [`named`] by `name`.
///
/// One function with one `C: Choice` bound, so an enum spelled by `#[arg(…)]`
/// without `#[derive(Choice)]` is a single E0277 — the trait's own message and
/// note — rather than that plus a `FromStr` error listing the standard
/// library's implementors.
pub fn choice<C: Choice, P: Parser<String>>(
    name: &'static str,
    help: &str,
    head: impl FnOnce(Doc) -> P,
) -> impl Parser<C> {
    head(choice_help::<C>(help))
        .complete(|w: &String| complete_choice::<C>(w))
        .parse(move |w: String| w.parse::<C>().map_err(|e| named(name, e)))
}

/// Parses one word into `T`; `.0` wraps a parsed field into `T`.
///
/// The receiver of the parse probe: `(&FromStrOf::<T, I>(wrap)).parse_word(w)`
/// picks [`SelfFromStr`] when `T: FromStr`, else [`FieldFromStr`].
pub struct FromStrOf<T, I>(pub fn(I) -> T);

/// The parse of a type with its own `FromStr`: the type's rule wins.
pub trait SelfFromStr {
    /// What the word parses into.
    type Value;
    /// What a word that does not parse is.
    type Error;
    /// Parse `word` through the probed type's own `FromStr`.
    fn parse_word(&self, word: &str) -> Result<Self::Value, Self::Error>;
}

impl<T: FromStr, I> SelfFromStr for FromStrOf<T, I> {
    type Value = T;
    type Error = T::Err;
    fn parse_word(&self, word: &str) -> Result<T, T::Err> {
        word.parse()
    }
}

/// The parse of a type without a `FromStr`: its field's, then wrapped.
pub trait FieldFromStr {
    /// What the word parses into.
    type Value;
    /// What a word that does not parse is.
    type Error;
    /// Parse `word` through the field's `FromStr` and wrap it.
    fn parse_word(&self, word: &str) -> Result<Self::Value, Self::Error>;
}

impl<T, I: FromStr> FieldFromStr for &FromStrOf<T, I> {
    type Value = T;
    type Error = I::Err;
    fn parse_word(&self, word: &str) -> Result<T, I::Err> {
        word.parse::<I>().map(self.0)
    }
}

/// The parse of a path-valued type with its own `FromStr`: the type's rule,
/// on the path's UTF-8 text.
pub trait SelfFromOs {
    /// The word as bpaf read it: a `PathBuf` or an `OsString`.
    type Word;
    /// What the word parses into.
    type Value;
    /// What a word that does not parse is.
    type Error;
    /// Parse `word` through the probed type's own `FromStr`.
    fn parse_os(&self, word: Self::Word) -> Result<Self::Value, Self::Error>;
}

impl<T: FromStr, I: AsRef<OsStr>> SelfFromOs for FromStrOf<T, I> {
    type Word = I;
    type Value = T;
    type Error = OsWordError<T::Err>;
    fn parse_os(&self, word: I) -> Result<T, OsWordError<T::Err>> {
        word.as_ref()
            .to_str()
            .ok_or(OsWordError::NotUtf8)?
            .parse()
            .map_err(OsWordError::Parse)
    }
}

/// The parse of a path-valued type without a `FromStr`: the path unchanged,
/// wrapped — non-UTF-8 bytes kept.
pub trait FieldFromOs {
    /// The word as bpaf read it: a `PathBuf` or an `OsString`.
    type Word;
    /// What the word parses into.
    type Value;
    /// What a word that does not parse is: nothing.
    type Error;
    /// Wrap `word`.
    fn parse_os(&self, word: Self::Word) -> Result<Self::Value, Self::Error>;
}

impl<T, I> FieldFromOs for &FromStrOf<T, I> {
    type Word = I;
    type Value = T;
    type Error = std::convert::Infallible;
    fn parse_os(&self, word: I) -> Result<T, std::convert::Infallible> {
        Ok((self.0)(word))
    }
}

/// Why a path word did not parse through its type's own `FromStr`.
#[derive(Debug)]
pub enum OsWordError<E> {
    /// The word is not UTF-8, so no `FromStr` can see it.
    NotUtf8,
    /// The type's `FromStr` rejected it.
    Parse(E),
}

impl<E: Display> Display for OsWordError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 => f.write_str("not valid UTF-8"),
            Self::Parse(e) => e.fmt(f),
        }
    }
}

/// A spelled parse error, prefixed with the argument's spelling (ADR-018):
/// `` `--limit`: must be at least 1 ``.
pub fn named(name: &'static str, error: impl Display) -> String {
    format!("`{name}`: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use super::{FieldFromOs as _, FieldFromStr as _, SelfFromOs as _, SelfFromStr as _};
    #[allow(unused_imports)]
    use super::{IsChoice as _, NotChoice as _};
    use crate::Shell;
    use std::path::PathBuf;

    /// A path that must name an existing file: its own `FromStr` says so.
    #[derive(Debug)]
    struct Existing(PathBuf);

    impl FromStr for Existing {
        type Err = String;
        fn from_str(s: &str) -> Result<Self, String> {
            let p = PathBuf::from(s);
            if p.is_file() {
                Ok(Existing(p))
            } else {
                Err("does not exist".to_string())
            }
        }
    }

    /// A title: no `FromStr` of its own.
    #[derive(Debug, PartialEq)]
    struct Title(String);

    /// A path: no `FromStr` of its own.
    #[derive(Debug, PartialEq)]
    struct Out(PathBuf);

    // The borrow is the mechanism under test, as for `Probe`.
    #[allow(clippy::needless_borrow)]
    #[test]
    fn the_parse_probe_picks_the_type_when_both_apply() {
        let e = (&FromStrOf::<Existing, PathBuf>(Existing)).parse_word("no/such");
        assert_eq!(e.expect_err("not a file"), "does not exist");
        let ok = (&FromStrOf::<Existing, PathBuf>(Existing)).parse_word("Cargo.toml");
        assert_eq!(ok.expect("a file").0, PathBuf::from("Cargo.toml"));
    }

    #[allow(clippy::needless_borrow)]
    #[test]
    fn the_parse_probe_picks_the_field_when_the_type_has_no_from_str() {
        let t = (&FromStrOf::<Title, String>(Title)).parse_word("x");
        assert_eq!(t, Ok(Title("x".to_string())));
    }

    #[allow(clippy::needless_borrow)]
    #[test]
    fn the_os_probe_picks_the_type_and_keeps_bytes_for_the_field() {
        let ok = (&FromStrOf::<Existing, PathBuf>(Existing)).parse_os(PathBuf::from("Cargo.toml"));
        assert!(ok.is_ok());
        let e = (&FromStrOf::<Existing, PathBuf>(Existing)).parse_os(PathBuf::from("no/such"));
        assert_eq!(e.expect_err("not a file").to_string(), "does not exist");
        let out = (&FromStrOf::<Out, PathBuf>(Out)).parse_os(PathBuf::from("x"));
        assert_eq!(out, Ok(Out(PathBuf::from("x"))));
    }

    #[cfg(unix)]
    #[allow(clippy::needless_borrow)]
    #[test]
    fn a_non_utf8_path_is_an_error_for_the_type_and_kept_for_the_field() {
        use std::os::unix::ffi::OsStrExt;
        let raw = PathBuf::from(OsStr::from_bytes(b"caf\xe9"));
        let e = (&FromStrOf::<Existing, PathBuf>(Existing)).parse_os(raw.clone());
        assert!(matches!(e, Err(OsWordError::NotUtf8)), "{e:?}");
        assert_eq!(e.expect_err("not UTF-8").to_string(), "not valid UTF-8");
        let out = (&FromStrOf::<Out, PathBuf>(Out)).parse_os(raw.clone());
        assert_eq!(out.expect("kept").0.as_os_str().as_bytes(), b"caf\xe9");
    }

    // The borrow is the mechanism under test: the generated code writes
    // `(&Probe::<T>::NEW).completer()`, and so must the test.
    #[allow(clippy::needless_borrow)]
    #[test]
    fn the_probe_picks_the_choice_completer_for_a_choice() {
        let complete = (&Probe::<Shell>::NEW).completer();
        assert_eq!(
            complete("").into_iter().map(|(n, _)| n).collect::<Vec<_>>(),
            ["bash", "zsh", "fish"]
        );
        assert_eq!(complete("z").len(), 1);
    }

    #[test]
    fn the_probe_picks_no_candidates_for_anything_else() {
        // Here the borrow is needed: `NotChoice` is implemented for
        // `&Probe<T>`, and the lookup reaches it by auto-referencing that.
        let complete = (&Probe::<String>::NEW).completer();
        assert!(complete("").is_empty());
        let complete = (&Probe::<u16>::NEW).completer();
        assert!(complete("8").is_empty());
        let complete = (&Probe::<std::path::PathBuf>::NEW).completer();
        assert!(complete("").is_empty());
    }

    #[allow(clippy::needless_borrow)]
    #[test]
    fn the_probe_writes_a_choice_help_row_with_its_names() {
        assert_eq!(
            (&Probe::<Shell>::NEW).help_row("x").monochrome(true),
            "x [bash, zsh, fish]"
        );
        assert_eq!((&Probe::<String>::NEW).help_row("x").monochrome(true), "x");
    }

    /// A choice with nothing to choose from.
    #[derive(Debug)]
    enum Nothing {}

    impl Choice for Nothing {
        const VARIANTS: &'static [Self] = &[];
        fn name(&self) -> &'static str {
            match *self {}
        }
        fn description(&self) -> Option<&'static str> {
            match *self {}
        }
    }

    impl FromStr for Nothing {
        type Err = crate::UnknownChoice;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            Err(crate::UnknownChoice::new::<Self>(s))
        }
    }

    impl Display for Nothing {
        fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
            match *self {}
        }
    }

    #[test]
    fn the_names_go_before_a_final_full_stop() {
        assert_eq!(
            choice_help::<Shell>("Never played takes precedence.").monochrome(true),
            "Never played takes precedence [bash, zsh, fish]."
        );
    }

    #[test]
    fn the_names_go_after_an_ellipsis_or_other_punctuation() {
        assert_eq!(
            choice_help::<Shell>("And more...").monochrome(true),
            "And more... [bash, zsh, fish]"
        );
        assert_eq!(
            choice_help::<Shell>("Which one?").monochrome(true),
            "Which one? [bash, zsh, fish]"
        );
        assert_eq!(
            choice_help::<Shell>("The shell").monochrome(true),
            "The shell [bash, zsh, fish]"
        );
    }

    #[test]
    fn a_choice_without_variants_gets_no_suffix() {
        assert_eq!(choice_help::<Nothing>("x").monochrome(true), "x");
    }

    #[test]
    fn named_prefixes_the_spelling() {
        assert_eq!(
            named("--limit", "must be at least 1"),
            "`--limit`: must be at least 1"
        );
    }

    #[test]
    fn choice_names_the_argument_in_its_error() {
        let e = choice::<Shell, _>("SHELL", "s", |h| {
            bpaf::positional::<String>("SHELL").help(h)
        })
        .to_options()
        .run_inner(&["tcsh"][..])
        .expect_err("not a shell")
        .unwrap_stderr();
        assert_eq!(
            e,
            "couldn't parse `tcsh`: `SHELL`: expected `bash`, `zsh` or `fish`"
        );
        let ok = choice::<Shell, _>("SHELL", "s", |h| {
            bpaf::positional::<String>("SHELL").help(h)
        })
        .to_options()
        .run_inner(&["zsh"][..])
        .expect("a shell");
        assert_eq!(ok, Shell::Zsh);
    }
}
