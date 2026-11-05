//! Help-text layout `bpaf` does not offer: a block printed exactly as written.
//!
//! `bpaf`'s documentation model is prose. Text is reflowed, a blank line is a
//! paragraph break — and plain `--help` prints only the **first** paragraph of
//! a header or footer — and a line indented by four or more spaces becomes a
//! "code line" whose indent is stripped and whose first line is glued to the
//! text before it. None of that is what an examples section wants, and `bpaf`
//! has no preformatted block, so hicli builds one itself (ADR-008).

use bpaf::doc::Doc;

/// `text` as a `bpaf` [`Doc`] that renders line for line: every line on its
/// own output line, its leading spaces kept, blank lines kept as blank lines,
/// in plain `--help` and in `--help --help` alike.
///
/// The encoding is one inline block per line, joined by `"\n "` — `bpaf`'s
/// hard line break. Each line goes in through its **own** `doc.doc(..)` call,
/// and that is load-bearing twice over:
///
/// - Appended as plain text, a line would merge with the `"\n "` before it
///   into one text token, and `"\n"` followed by four or more spaces is
///   `bpaf`'s code-line syntax — the indent would be stripped and the line
///   glued to its predecessor.
/// - A blank line is *nothing* between two hard breaks, never `"\n\n"`:
///   `"\n\n"` is a paragraph break, and plain `--help` stops printing the
///   footer at the first one.
///
/// Lines longer than `bpaf`'s 100-column width still wrap, and a wrapped
/// continuation starts at column zero — which is why the crate docs advise
/// keeping examples under 100 columns.
pub(crate) fn verbatim(text: &str) -> Doc {
    let mut doc = Doc::default();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            doc.text("\n ");
        }
        if !line.is_empty() {
            doc.doc(&Doc::from(line));
        }
    }
    doc
}

/// A usage line for `OptionParser::with_usage`: `Usage:`, then the `shown`
/// words — the binary's name, the command path, each curried value's metavar
/// — then the part `bpaf` derives from the parser's own items (ADR-013).
///
/// Every gap is one space, and there is never a trailing one: the derived
/// part is appended only when it renders to something. `bpaf`'s own line
/// writes "word + space" per path word, so a command without arguments ends
/// in a space (#28); and it names the binary only via `Args::set_name`, which
/// hicli never calls (ADR-009 §3), so it names none (#21).
///
/// Styled as `bpaf` styles its own: `Usage` emphasised, the words literal.
pub(crate) fn usage_line(shown: Vec<String>) -> impl Fn(Doc) -> Doc {
    move |derived| {
        let mut u = Doc::default();
        u.emphasis("Usage");
        u.text(":");
        for word in &shown {
            u.text(" ");
            u.literal(word);
        }
        if !derived.monochrome(false).is_empty() {
            u.text(" ");
            u.doc(&derived);
        }
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_usage_line_without_a_derived_part_has_no_trailing_space() {
        let line = usage_line(words(&["playr", "playback", "stop"]))(Doc::default());
        assert_eq!(line.monochrome(false), "Usage: playr playback stop");
    }

    #[test]
    fn a_usage_line_without_words_is_usage_and_the_derived_part() {
        let mut derived = Doc::default();
        derived.text("[--node=HOST] COMMAND ...");
        let line = usage_line(Vec::new())(derived);
        assert_eq!(line.monochrome(false), "Usage: [--node=HOST] COMMAND ...");
    }

    #[test]
    fn a_usage_line_joins_words_and_the_derived_part_by_one_space() {
        let mut derived = Doc::default();
        derived.text("NAME");
        let line = usage_line(words(&["playr", "set-output"]))(derived);
        assert_eq!(line.monochrome(false), "Usage: playr set-output NAME");
    }

    /// A footer exercising every hazard: a heading, a two-space indent, a
    /// blank line, and a line indented by more than four spaces.
    const TEXT: &str =
        "Examples:\n  demo search --artist Bowie\n\n      demo search --added -7\nend";

    #[test]
    fn verbatim_renders_back_to_its_lines_in_both_renderings() {
        assert_eq!(verbatim(TEXT).monochrome(false), TEXT);
        assert_eq!(verbatim(TEXT).monochrome(true), TEXT);
    }

    #[test]
    fn verbatim_of_one_line_is_that_line() {
        assert_eq!(verbatim("Examples:").monochrome(false), "Examples:");
    }
}
