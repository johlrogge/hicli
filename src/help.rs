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

#[cfg(test)]
mod tests {
    use super::*;

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
