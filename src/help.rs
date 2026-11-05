//! Help-text layout `bpaf` does not offer: a block printed exactly as written.
//!
//! `bpaf`'s documentation model is prose. Text is reflowed, a blank line is a
//! paragraph break — and plain `--help` prints only the **first** paragraph of
//! a header or footer — and a line indented by four or more spaces becomes a
//! "code line" whose indent is stripped and whose first line is glued to the
//! text before it. None of that is what an examples section wants, and `bpaf`
//! has no preformatted block, so hicli builds one itself (ADR-008).

use bpaf::doc::{Doc, Style};

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
///
/// # Long lines (#30, ADR-013 §4)
///
/// A line that fits in [`WIDTH`] columns is built from `bpaf`'s own tokens,
/// as above. A longer one would be wrapped by `bpaf`'s renderer, which breaks
/// wherever a chunk overflows — and chunks end at style boundaries as well as
/// at spaces, so a `[` or a `]` can land on a line apart from its option, and
/// the continuation starts at column zero. So hicli wraps it first: the
/// derived part's text is laid out between items ([`layout`]), every
/// continuation is indented to where the derived part starts (or 7 columns,
/// under `Usage: `, when that is past half the width), and each line is
/// re-styled from its text by `bpaf`'s usage grammar ([`styled`]). The text is
/// all `bpaf`'s; only its styling is rebuilt, and that can mis-style a metavar
/// containing a space or a bracket, or a custom `any` metavar — on lines over
/// 100 columns only.
pub(crate) fn usage_line(shown: Vec<String>, bare_default: bool) -> impl Fn(Doc) -> Doc {
    move |derived| {
        let mut u = Doc::default();
        u.emphasis("Usage");
        u.text(":");
        for word in &shown {
            u.text(" ");
            u.literal(word);
        }
        // The derived part's text, unwrapped: `Display` renders monochrome at
        // the formatter's width.
        let text = format!("{:w$}", derived, w = u16::MAX as usize);
        if text.is_empty() {
            return u;
        }
        let text = if bare_default {
            format!("[{text}]")
        } else {
            text
        };
        let prefix = "Usage:".len() + shown.iter().map(|w| 1 + w.len()).sum::<usize>();
        if prefix + 1 + text.len() <= WIDTH && !text.contains('\n') {
            u.text(" ");
            if bare_default {
                u.text("[");
                u.doc(&derived);
                u.text("]");
            } else {
                u.doc(&derived);
            }
            return u;
        }
        // `prefix + 1`, where the derived part starts, if that is at most half
        // the width.
        let indent = if prefix < WIDTH / 2 {
            prefix + 1
        } else {
            "Usage: ".len()
        };
        let first = WIDTH.saturating_sub(prefix + 1);
        for (i, line) in layout(&text, first, WIDTH - indent).iter().enumerate() {
            if i == 0 {
                if !line.is_empty() {
                    u.text(" ");
                    u.doc(&Doc::from(&styled(line)[..]));
                }
                continue;
            }
            // `"\n "` is `bpaf`'s hard line break. The indent is a `Doc` of
            // its own so it never merges with it: `"\n"` followed by four or
            // more spaces is a code line, whose indent `bpaf` strips.
            u.text("\n ");
            u.doc(&Doc::from(" ".repeat(indent).as_str()));
            u.doc(&Doc::from(&styled(line)[..]));
        }
        u
    }
}

/// The lines a usage line's derived `text` wraps into: the first at most
/// `first` bytes (it follows `Usage:` and the shown words), every other at
/// most `rest`. Bytes, because they are what `bpaf`'s overflow check measures.
///
/// Lines break only at spaces, and never inside a unit: a `...` word stays
/// with the word before it (`COMMAND ...`), a `|` word with the word after it
/// (`| [--artist=ARTIST]`). Units are filled greedily, one space apart. A unit
/// longer than a line gets a line of its own; one that does not fit beside the
/// prefix, but would on a line of its own, starts the second line — the first
/// is then empty.
pub(crate) fn layout(text: &str, first: usize, rest: usize) -> Vec<String> {
    let mut units: Vec<String> = Vec::new();
    let mut bar = false;
    for word in text.split([' ', '\n']).filter(|w| !w.is_empty()) {
        if word == "..." {
            if let Some(last) = units.last_mut() {
                last.push_str(" ...");
                continue;
            }
        }
        if bar {
            if let Some(last) = units.last_mut() {
                last.push(' ');
                last.push_str(word);
            }
            bar = false;
            continue;
        }
        units.push(word.to_string());
        bar = word == "|";
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for unit in units {
        let budget = if lines.is_empty() { first } else { rest };
        if current.is_empty() {
            if lines.is_empty() && unit.len() > first && first < rest {
                lines.push(String::new());
            }
            current = unit;
        } else if current.len() + 1 + unit.len() <= budget {
            current.push(' ');
            current.push_str(&unit);
        } else {
            lines.push(std::mem::replace(&mut current, unit));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// `line` — a piece of a usage line's derived part — as styled spans, by the
/// grammar `bpaf` writes usage in (`Doc::write_meta`): `COMMAND ...` and
/// metavars are [`Style::Metavar`]; brackets, parentheses, `|`, spaces, `...`
/// and an option's `=` are [`Style::Text`]; an option's name, a run that
/// starts with `-` up to `=`, a space, `]`, `)`, `|` or `...`, is
/// [`Style::Literal`]. Adjacent spans of one style are merged, as a [`Doc`]
/// merges them.
pub(crate) fn styled(line: &str) -> Vec<(&str, Style)> {
    /// The end of a run starting at `from`: the first space, `]`, `)`, `|`,
    /// `...` or, if `eq`, `=`.
    fn run_end(line: &str, from: usize, eq: bool) -> usize {
        let mut end = from;
        for c in line[from..].chars() {
            let rest = &line[end..];
            if rest.starts_with([' ', ']', ')', '|']) || rest.starts_with("...") || (eq && c == '=')
            {
                break;
            }
            end += c.len_utf8();
        }
        end
    }
    fn push(spans: &mut Vec<(usize, usize, Style)>, start: usize, end: usize, style: Style) {
        if start == end {
            return;
        }
        match spans.last_mut() {
            Some(last) if last.2 == style && last.1 == start => last.1 = end,
            _ => spans.push((start, end, style)),
        }
    }

    let mut spans = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        if rest.starts_with("COMMAND ...") {
            let end = i + "COMMAND ...".len();
            push(&mut spans, i, end, Style::Metavar);
            i = end;
        } else if rest.starts_with("...") {
            push(&mut spans, i, i + 3, Style::Text);
            i += 3;
        } else if rest.starts_with([' ', '[', ']', '(', ')', '|']) {
            push(&mut spans, i, i + 1, Style::Text);
            i += 1;
        } else if rest.starts_with('-') {
            let end = run_end(line, i, true);
            push(&mut spans, i, end, Style::Literal);
            i = end;
            if line[i..].starts_with('=') {
                push(&mut spans, i, i + 1, Style::Text);
                i += 1;
                let end = run_end(line, i, false);
                push(&mut spans, i, end, Style::Metavar);
                i = end;
            }
        } else {
            let end = run_end(line, i, false);
            push(&mut spans, i, end, Style::Metavar);
            i = end;
        }
    }
    spans
        .into_iter()
        .map(|(start, end, style)| (&line[start..end], style))
        .collect()
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

    // --- #30: long usage lines wrap between items --------------------------------

    #[test]
    fn layout_breaks_only_at_spaces() {
        let text = "[--alpha=A] [--bravo=B] [--charlie=C]";
        assert_eq!(
            layout(text, 24, 24),
            ["[--alpha=A] [--bravo=B]", "[--charlie=C]"]
        );
        for line in layout(text, 24, 24) {
            assert!(line.len() <= 24, "{line:?}");
        }
    }

    #[test]
    fn layout_keeps_command_and_its_dots_together() {
        // `COMMAND` alone fits the first line; `COMMAND ...` does not.
        assert_eq!(
            layout("[--node=HOST] COMMAND ...", 21, 21),
            ["[--node=HOST]", "COMMAND ..."]
        );
    }

    #[test]
    fn layout_starts_a_line_with_a_bar() {
        // `(COMMAND ... |` would fit the first line; the `|` goes with what
        // follows it instead.
        assert_eq!(
            layout("(COMMAND ... | [--artist=ARTIST] [QUERY])", 20, 30),
            ["(COMMAND ...", "| [--artist=ARTIST] [QUERY])"]
        );
    }

    #[test]
    fn layout_gives_an_over_long_unit_a_line_of_its_own() {
        assert_eq!(
            layout("[-v] [--a-very-long-option=VALUE] [-q]", 10, 10),
            ["[-v]", "[--a-very-long-option=VALUE]", "[-q]"]
        );
    }

    #[test]
    fn layout_starts_on_the_next_line_when_the_first_unit_does_not_fit_beside_the_prefix() {
        assert_eq!(layout("[--alpha=A] [-b]", 5, 20), ["", "[--alpha=A] [-b]"]);
    }

    #[test]
    fn layout_of_a_fitting_text_is_one_line() {
        assert_eq!(layout("[-v] FILE", 9, 9), ["[-v] FILE"]);
    }

    #[test]
    fn styled_spells_an_option_its_value_and_a_repeat() {
        assert_eq!(
            styled("[--playlist=PLAYLIST]..."),
            [
                ("[", Style::Text),
                ("--playlist", Style::Literal),
                ("=", Style::Text),
                ("PLAYLIST", Style::Metavar),
                ("]...", Style::Text),
            ]
        );
    }

    #[test]
    fn styled_spells_command_alternatives_and_a_positional() {
        assert_eq!(
            styled("(COMMAND ... | [QUERY])"),
            [
                ("(", Style::Text),
                ("COMMAND ...", Style::Metavar),
                (" | [", Style::Text),
                ("QUERY", Style::Metavar),
                ("])", Style::Text),
            ]
        );
    }

    #[test]
    fn styled_spells_a_strict_double_dash() {
        assert_eq!(
            styled("-- FILE"),
            [
                ("--", Style::Literal),
                (" ", Style::Text),
                ("FILE", Style::Metavar)
            ]
        );
    }

    #[test]
    fn styled_spells_a_short_switch() {
        assert_eq!(
            styled("[-v]"),
            [
                ("[", Style::Text),
                ("-v", Style::Literal),
                ("]", Style::Text)
            ]
        );
    }

    #[test]
    fn styled_renders_back_to_its_line() {
        let line = "(COMMAND ... | [--artist=ARTIST] [-v] <file>... -- FILE)";
        let spans = styled(line);
        assert_eq!(spans.iter().map(|(s, _)| *s).collect::<String>(), line);
        assert_eq!(Doc::from(&spans[..]).monochrome(true), line);
    }

    /// A derived part of `n` bytes: `[-a]`-style switches, then `A`s to pad.
    fn derived_of(n: usize) -> Doc {
        let mut text = String::new();
        while text.len() + 5 <= n {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str("[-a]");
        }
        while text.len() < n {
            text.push('A');
        }
        assert_eq!(text.len(), n);
        let mut d = Doc::default();
        d.text(&text);
        d
    }

    #[test]
    fn a_usage_line_of_exactly_the_width_stays_on_one_line() {
        // `Usage: app ` is 11 bytes.
        let line = usage_line(words(&["app"]), false)(derived_of(WIDTH - 11)).monochrome(true);
        assert_eq!(line.len(), WIDTH, "{line}");
        assert!(!line.contains('\n'), "{line}");
    }

    #[test]
    fn a_usage_line_one_over_the_width_wraps_under_the_derived_part() {
        let line = usage_line(words(&["app"]), false)(derived_of(WIDTH - 10)).monochrome(true);
        let lines: Vec<&str> = line.lines().collect();
        assert_eq!(lines.len(), 2, "{line}");
        assert!(lines.iter().all(|l| l.len() <= WIDTH), "{line}");
        // The continuation is indented to where the derived part starts.
        let indent = "Usage: app ".len();
        assert_eq!(&lines[1][..indent], " ".repeat(indent), "{line}");
        assert!(!lines[1][indent..].starts_with(' '), "{line}");
        assert_eq!(
            lines[0].to_string() + " " + &lines[1][indent..],
            "Usage: app ".to_string() + &derived_of(WIDTH - 10).monochrome(true)
        );
    }

    #[test]
    fn a_wrapped_bare_defaults_usage_line_keeps_its_brackets() {
        let mut derived = Doc::default();
        derived.text(&["[--option=VALUE]"; 6].join(" ")); // 101 bytes
        let line = usage_line(words(&["app"]), true)(derived).monochrome(true);
        assert!(line.starts_with("Usage: app [[--option=VALUE] "), "{line}");
        assert!(line.ends_with(" [--option=VALUE]]"), "{line}");
        assert!(line.lines().all(|l| l.len() <= WIDTH), "{line}");
    }

    #[test]
    fn a_big_prefix_falls_back_to_an_indent_of_seven() {
        let long = "a".repeat(50);
        // `Usage: <50> ` is 57 bytes: past half the width.
        let mut derived = Doc::default();
        derived.text(&["[--option=VALUE]"; 4].join(" "));
        let line = usage_line(words(&[&long]), false)(derived).monochrome(true);
        let lines: Vec<&str> = line.lines().collect();
        assert_eq!(
            lines,
            [
                format!("Usage: {long} [--option=VALUE] [--option=VALUE]"),
                "       [--option=VALUE] [--option=VALUE]".to_string(),
            ],
            "{line}"
        );
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
