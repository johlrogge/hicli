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
//! use ::hicli::__private::{ChoiceCompleter as _, NoCompleter as _};
//! let complete = (&::hicli::__private::Probe::<SortField>::NEW).completer();
//! ```
//!
//! The receiver is `&Probe<T>`. Method lookup tries the receiver's own type
//! first, where only [`ChoiceCompleter`] has a method (its `&self` *is*
//! `&Probe<T>`), and that impl applies only when `T: Choice`. Failing that,
//! lookup auto-references to `&&Probe<T>`, where [`NoCompleter`]'s impl for
//! `&Probe<T>` always applies. The type must be concrete at the call, which it
//! is in every expansion.

use std::marker::PhantomData;

use bpaf::Parser;

use crate::arg::Candidate;
use crate::choice::{complete_choice, Choice};

/// A value that only carries a type, for method resolution to look at.
pub struct Probe<T>(PhantomData<T>);

impl<T> Probe<T> {
    /// The probe for `T`.
    pub const NEW: Self = Probe(PhantomData);
}

/// The completer of a type that is a [`Choice`]: its names.
pub trait ChoiceCompleter {
    /// The completer for the probed type.
    fn completer(&self) -> fn(&str) -> Vec<Candidate>;
}

impl<T: Choice> ChoiceCompleter for Probe<T> {
    fn completer(&self) -> fn(&str) -> Vec<Candidate> {
        complete_choice::<T>
    }
}

/// The completer of every other type: nothing.
pub trait NoCompleter {
    /// The completer for the probed type.
    fn completer(&self) -> fn(&str) -> Vec<Candidate>;
}

impl<T> NoCompleter for &Probe<T> {
    fn completer(&self) -> fn(&str) -> Vec<Candidate> {
        no_candidates
    }
}

/// The completer that offers nothing.
pub fn no_candidates(_: &str) -> Vec<Candidate> {
    Vec::new()
}

/// The tail of a self-spelled choice enum's chain: complete `word` with `C`'s
/// names, then parse it as a `C`.
///
/// One function with one `C: Choice` bound, so an enum spelled by `#[arg(…)]`
/// without `#[derive(Choice)]` is a single E0277 — the trait's own message and
/// note — rather than that plus a `FromStr` error listing the standard
/// library's implementors.
pub fn choice<C: Choice>(word: impl Parser<String>) -> impl Parser<C> {
    word.complete(|w: &String| complete_choice::<C>(w))
        .parse(|w: String| w.parse::<C>())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use super::{ChoiceCompleter as _, NoCompleter as _};
    use crate::Shell;

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
        // Here the borrow is needed: `NoCompleter` is implemented for
        // `&Probe<T>`, and the lookup reaches it by auto-referencing that.
        let complete = (&Probe::<String>::NEW).completer();
        assert!(complete("").is_empty());
        let complete = (&Probe::<u16>::NEW).completer();
        assert!(complete("8").is_empty());
        let complete = (&Probe::<std::path::PathBuf>::NEW).completer();
        assert!(complete("").is_empty());
    }
}
