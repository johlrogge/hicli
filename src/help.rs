//! Help-text layout `bpaf` does not offer: a block printed exactly as written.
//!
//! `bpaf`'s documentation model is prose. Text is reflowed, a blank line is a
//! paragraph break — and plain `--help` prints only the **first** paragraph of
//! a header or footer — and a line indented by four or more spaces becomes a
//! "code line" whose indent is stripped and whose first line is glued to the
//! text before it. None of that is what an examples section wants, and `bpaf`
//! has no preformatted block, so hicli builds one itself (ADR-008).

use bpaf::doc::Doc;

/// The width hicli renders at when it prints through
/// `ParseFailure::print_message`: `bpaf`'s default `Info::max_width`
/// (`info.rs`), which hicli never changes — so a printed message wraps where
/// `bpaf`'s own `monochrome` rendering, and therefore every test, wraps it.
pub(crate) const WIDTH: usize = 100;

/// `msg`, then `did you mean `<name>`?` on a line of its own — the
/// suggestion gap-closer's hint appended to `bpaf`'s error as a [`Doc`], so it
/// is printed through the same (possibly coloured) path, the name styled as a
/// literal.
///
/// `msg` goes in through its own `doc.doc(..)`, so its last token never merges
/// with the `"\n "` after it, and `"\n "` is `bpaf`'s hard line break: a lone
/// `"\n"` would be reflowed into a space. Rendered monochrome, the result is
/// byte for byte `"<msg>\ndid you mean `<name>`?"`.
pub(crate) fn with_suggestion(msg: &Doc, name: &str) -> Doc {
    let mut doc = Doc::default();
    doc.doc(msg);
    doc.text("\n did you mean `");
    doc.literal(name);
    doc.text("`?");
    doc
}

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
///
/// `bare_default` is set for a group whose default command takes no
/// arguments (#19): naming a command is then optional, but `bpaf` drops an
/// alternative with no items from the derived part, which says `COMMAND ...`
/// as if one were required. The derived part is bracketed, `[COMMAND ...]`.
pub(crate) fn usage_line(shown: Vec<String>, bare_default: bool) -> impl Fn(Doc) -> Doc {
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
            if bare_default {
                u.text("[");
                u.doc(&derived);
                u.text("]");
            } else {
                u.doc(&derived);
            }
        }
        u
    }
}

/// The header of a group whose default command takes no arguments (#19): the
/// default's one-line `help`, a blank line, then the group's `details`.
///
/// `bpaf` lists a default's help line only as the title of its options, so a
/// default without any would never be described. The line comes first because
/// plain `--help` prints only a header's first paragraph (ADR-008); the blank
/// line is two hard breaks, never `"\n\n"`, so it does not end that paragraph.
pub(crate) fn default_header(help: &str, details: Option<&str>) -> Doc {
    let mut doc = Doc::default();
    doc.doc(&Doc::from(help));
    if let Some(details) = details {
        doc.text("\n \n ");
        doc.doc(&Doc::from(details));
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ws: &[&str]) -> Vec<String> {
        ws.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_usage_line_without_a_derived_part_has_no_trailing_space() {
        let line = usage_line(words(&["playr", "playback", "stop"]), false)(Doc::default());
        assert_eq!(line.monochrome(false), "Usage: playr playback stop");
    }

    #[test]
    fn a_usage_line_without_words_is_usage_and_the_derived_part() {
        let mut derived = Doc::default();
        derived.text("[--node=HOST] COMMAND ...");
        let line = usage_line(Vec::new(), false)(derived);
        assert_eq!(line.monochrome(false), "Usage: [--node=HOST] COMMAND ...");
    }

    #[test]
    fn a_usage_line_joins_words_and_the_derived_part_by_one_space() {
        let mut derived = Doc::default();
        derived.text("NAME");
        let line = usage_line(words(&["playr", "set-output"]), false)(derived);
        assert_eq!(line.monochrome(false), "Usage: playr set-output NAME");
    }

    #[test]
    fn a_bare_defaults_usage_line_brackets_the_derived_part() {
        let mut derived = Doc::default();
        derived.text("COMMAND ...");
        let line = usage_line(words(&["playr", "library", "archive"]), true)(derived);
        assert_eq!(
            line.monochrome(false),
            "Usage: playr library archive [COMMAND ...]"
        );
    }

    #[test]
    fn a_bare_defaults_usage_line_without_a_derived_part_is_unbracketed() {
        let line = usage_line(words(&["archive"]), true)(Doc::default());
        assert_eq!(line.monochrome(false), "Usage: archive");
    }

    #[test]
    fn a_default_header_is_the_line_a_blank_line_and_the_details() {
        let h = default_header("list the archive", Some("about"));
        assert_eq!(h.monochrome(false), "list the archive\n\nabout");
        assert_eq!(default_header("list", None).monochrome(false), "list");
    }

    /// A footer exercising every hazard: a heading, a two-space indent, a
    /// blank line, and a line indented by more than four spaces.
    const TEXT: &str =
        "Examples:\n  demo search --artist Bowie\n\n      demo search --added=-7\nend";

    #[test]
    fn verbatim_renders_back_to_its_lines_in_both_renderings() {
        assert_eq!(verbatim(TEXT).monochrome(false), TEXT);
        assert_eq!(verbatim(TEXT).monochrome(true), TEXT);
    }

    #[test]
    fn verbatim_of_one_line_is_that_line() {
        assert_eq!(verbatim("Examples:").monochrome(false), "Examples:");
    }

    #[test]
    fn with_suggestion_puts_the_hint_on_its_own_line() {
        let doc = with_suggestion(&Doc::from("no such command"), "status");
        assert_eq!(
            doc.monochrome(true),
            "no such command\ndid you mean `status`?"
        );
    }

    /// A command's `Failure` message is printed through `verbatim` (ADR-015):
    /// a newline, a blank line, leading and inner runs of spaces, and a
    /// four-space indent — `bpaf`'s code-line syntax — all come out as
    /// written, in the rendering `print_message` uses for stderr (`full`).
    #[test]
    fn a_failure_message_renders_exactly_as_written() {
        let msg = "could not load the library:\n  /music: no such directory\n\n    \
                   try `playr init`\nfields:  a  b";
        assert_eq!(verbatim(msg).monochrome(true), msg);
        assert_eq!(verbatim("boom").monochrome(true), "boom");
    }

    /// The two ways a printed `Failure` differs from its message: a trailing
    /// newline is dropped (`print_message` ends the line itself), and a line
    /// longer than `WIDTH` wraps at a space, its continuation at column zero.
    #[test]
    fn a_failure_message_loses_a_trailing_newline_and_wraps_long_lines() {
        assert_eq!(verbatim("boom\n").monochrome(true), "boom");
        let long = ["word"; 30].join(" "); // 149 columns
        let rendered = verbatim(&long).monochrome(true);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 2, "{rendered}");
        assert!(lines.iter().all(|l| l.len() <= WIDTH), "{rendered}");
        assert_eq!(lines.join(" "), long);
    }
}
