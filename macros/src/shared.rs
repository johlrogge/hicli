//! The reading rules `#[command]` and `#[group]` share.
//!
//! A command and a group are the same kind of thing seen from the registration
//! side: an item with an ident (which becomes a command name **and** a handle
//! name), a doc comment (which becomes the help and the details) and possibly a
//! `#[cfg]` (which the handle has to inherit, or a configured-out item leaves a
//! handle behind). Those rules live here, once — ADR-005 §7 removes the second
//! spelling of the dash rule and the doc split that used to sit in the runtime
//! crate, precisely because two spellings can drift.

use proc_macro2::Span;
use quote::format_ident;
use quote::ToTokens;
use syn::ext::IdentExt;
use syn::parse::ParseStream;
use syn::{Attribute, Error, Expr, ExprLit, Ident, Lit, LitStr, Meta, Result, Token};

/// The message for a `#[command]` with no doc comment, or a blank one.
pub(crate) const COMMAND_NEEDS_DOC: &str =
    "a #[command] needs a doc comment: its first line is the command's help";

/// The same rule for a `#[group]` (ADR-001, ADR-005 §3).
pub(crate) const GROUP_NEEDS_DOC: &str =
    "a #[group] needs a doc comment: its first line is the group's help";

/// The command-line name an ident denotes: `add_admin` -> `add-admin`.
///
/// `unraw` first, so `mod r#move` is the command `move` — `r#` is Rust's
/// spelling problem, not the user's.
pub(crate) fn dashed(ident: &Ident) -> String {
    ident.unraw().to_string().replace('_', "-")
}

/// The name an ident denotes on disk: `mod r#type;` lives in `type.rs`.
///
/// The `r#` is Rust's spelling problem, as it is for [`dashed`] — the file is
/// named after the module, not after the escape.
pub(crate) fn unraw_name(ident: &Ident) -> String {
    ident.unraw().to_string()
}

/// The registration handle's ident: `__hicli_` + the item's, unescaped.
///
/// Spanned at the ident it was derived from, so an error about the handle
/// points at the item the author wrote.
pub(crate) fn handle_ident(ident: &Ident) -> Ident {
    format_ident!("__hicli_{}", ident.unraw(), span = ident.span())
}

/// The handle's own one-line doc.
///
/// It is `#[doc(hidden)]`, so this is not for a reader of the rendered docs —
/// it is for whoever lands on the item in an expansion, an error message or a
/// `go to definition`, and has to learn in one line what it is and where it
/// came from. It also keeps `missing_docs` quiet in the consumer's crate.
pub(crate) fn handle_doc(ident: &Ident) -> String {
    format!("`hicli` registration handle for [`{ident}`].")
}

/// Is this a `#[cfg(…)]` or `#[cfg_attr(…)]`?
pub(crate) fn is_cfg(attr: &Attribute) -> bool {
    let path = attr.path();
    path.is_ident("cfg") || path.is_ident("cfg_attr")
}

/// Does this attribute's path **end** in `name`?
///
/// `#[command]`, `#[hicli::command]` and `#[crate::command]` are all the same
/// mark: a proc macro sees tokens, not resolved paths, so the last segment is
/// as much as it can ask. A deliberate over-approximation — guessing wrong
/// produces a missing-function error naming `__hicli_x`, not silence.
pub(crate) fn marks(attr: &Attribute, name: &str) -> bool {
    attr.path().segments.last().is_some_and(|s| s.ident == name)
}

/// The `#[path = "…"]` an author wrote on a `mod` declaration, if any.
///
/// `#[group]` on a file module synthesises one wherever rustc's own rules would
/// not find the file it resolved, so
/// this is the question *"did the author already answer this?"* — and when they
/// did, their attribute is re-emitted untouched and means what it always meant:
/// a path relative to the declaring file's directory.
pub(crate) fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        let Meta::NameValue(nv) = &attr.meta else {
            return None;
        };
        if !nv.path.is_ident("path") {
            return None;
        }
        match &nv.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) => Some(s.value()),
            _ => None,
        }
    })
}

/// The `#[cfg]` / `#[cfg_attr]` attributes of an item, in source order.
pub(crate) fn cfg_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs.iter().filter(|a| is_cfg(a)).cloned().collect()
}

/// The text of one `#[doc = "..."]` attribute, if that is what it is.
pub(crate) fn doc_text(attr: &Attribute) -> Option<String> {
    let Meta::NameValue(nv) = &attr.meta else {
        return None;
    };
    if !nv.path.is_ident("doc") {
        return None;
    }
    match &nv.value {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Some(s.value()),
        _ => None,
    }
}

/// The message for an examples heading with nothing under it.
pub(crate) const EXAMPLES_EMPTY: &str = "this examples section is empty: put at least one \
                                         example under the heading, or remove the heading";

/// The three slots a doc comment fills (ADR-008).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DocParts {
    /// The first line: the one-line help, and the heading of `--help`.
    pub(crate) help: String,
    /// The prose between the help line and the examples heading — `bpaf`'s
    /// header, which reflows.
    pub(crate) details: Option<String>,
    /// The examples section, already laid out: `Examples:` and the body
    /// indented by two — rendered verbatim after the options.
    pub(crate) footer: Option<String>,
}

/// Is this line the heading that starts the examples footer?
///
/// Trimmed, exactly `# Examples` (rustdoc's convention) or `Examples:` (the
/// terminal's). `## Examples` is a sub-heading of something else, not this
/// slot.
fn is_examples_heading(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "# Examples" || trimmed == "Examples:"
}

/// The number of leading spaces and tabs on `line`.
fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// `line` with its first `n` characters of indentation removed — fewer, if it
/// has fewer.
fn dedent(line: &str, n: usize) -> &str {
    let cut = indent_of(line).min(n);
    // Indentation is ASCII, so a character count is a byte count.
    &line[cut..]
}

/// The smallest indentation among the non-blank `lines`, `0` if all are blank.
fn min_indent<'a>(lines: impl IntoIterator<Item = &'a str>) -> usize {
    lines
        .into_iter()
        .filter(|l| !l.trim().is_empty())
        .map(indent_of)
        .min()
        .unwrap_or(0)
}

/// Split the doc comment into the one-line help, the details and the examples
/// footer.
///
/// `attrs` is the item's whole attribute list, in source order: the doc
/// attributes are picked out of it and everything else ignored, because the
/// item is re-emitted with its attributes exactly as written.
///
/// **Help and details:** each line is trimmed (a `///` comment's text starts
/// with a space), leading blank lines are dropped, and the blank separator
/// between the first line and the rest disappears with them. A comment that is
/// missing *or* entirely blank fails with `missing`, spanned at `at` — the
/// token the author has to put a doc comment on.
///
/// **Footer:** the details stop at the first line after the help whose trimmed
/// text is exactly `# Examples` or `Examples:`; everything after it is the
/// examples body, read *without* trimming — see [`footer_of`].
pub(crate) fn read_doc<T: ToTokens>(
    attrs: &[Attribute],
    at: &T,
    missing: &str,
) -> Result<DocParts> {
    // One entry per line, each with the attribute it came from (for spans). A
    // `#[doc = "a\nb"]` is several lines; `split` rather than `lines` so an
    // empty `///` is still one (blank) line.
    let lines: Vec<(&Attribute, String)> = attrs
        .iter()
        .filter_map(|a| doc_text(a).map(|t| (a, t)))
        .flat_map(|(a, t)| {
            t.split('\n')
                .map(|l| (a, l.trim_end_matches('\r').to_string()))
                .collect::<Vec<_>>()
        })
        .collect();

    let Some(first) = lines.iter().position(|(_, l)| !l.trim().is_empty()) else {
        return Err(Error::new_spanned(at, missing));
    };
    let help = lines[first].1.trim().to_string();
    let rest = &lines[first + 1..];

    let heading = rest.iter().position(|(_, l)| is_examples_heading(l));
    let prose = match heading {
        Some(h) => &rest[..h],
        None => rest,
    };
    let prose: Vec<&str> = prose
        .iter()
        .map(|(_, l)| l.trim())
        .skip_while(|l| l.is_empty())
        .collect();
    let details = prose.join("\n").trim_end().to_string();

    let footer = match heading {
        Some(h) => {
            // Raw lines: `footer_of` removes the body's own common indentation.
            let body: Vec<&str> = rest[h + 1..].iter().map(|(_, l)| l.as_str()).collect();
            match footer_of(&body) {
                Some(f) => Some(f),
                None => return Err(Error::new_spanned(rest[h].0, EXAMPLES_EMPTY)),
            }
        }
        None => None,
    };

    Ok(DocParts {
        help,
        details: (!details.is_empty()).then_some(details),
        footer,
    })
}

/// Lay out an examples body as the footer `bpaf` will print verbatim, or
/// `None` when there is nothing in it.
///
/// Code fences (any line whose trimmed text starts with ```` ``` ````) are
/// dropped, so a ```` ```sh ```` block reads as plain lines; leading and
/// trailing blank lines are dropped; the body's common indentation is removed
/// and every non-blank line is indented by two instead; interior blank lines
/// are kept. The heading is always `Examples:`, however it was spelled.
fn footer_of(body: &[&str]) -> Option<String> {
    let kept: Vec<&str> = body
        .iter()
        .copied()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect();
    let start = kept.iter().position(|l| !l.trim().is_empty())?;
    let end = kept.iter().rposition(|l| !l.trim().is_empty())? + 1;
    let kept = &kept[start..end];
    let common = min_indent(kept.iter().copied());
    let laid_out: Vec<String> = kept
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("  {}", dedent(l, common).trim_end())
            }
        })
        .collect();
    Some(format!("Examples:\n{}", laid_out.join("\n")))
}

/// The registration keys `#[command]` and `#[group]` share: how a command is
/// *spelled* and whether it is *listed* (ADR-008).
#[derive(Default)]
pub(crate) struct CommandMeta {
    /// Every `alias = "…"`, in source order.
    pub(crate) aliases: Vec<LitStr>,
    /// Where `hide` was written, if it was.
    pub(crate) hide: Option<Span>,
}

impl CommandMeta {
    /// Read `key` (and its value, from `input`) if it is one of the shared
    /// keys. `Ok(false)` means "not mine" — the caller's to read or reject.
    fn read(&mut self, key: &Ident, input: ParseStream) -> Result<bool> {
        if key == "alias" {
            let _: Token![=] = input.parse()?;
            let alias: LitStr = input.parse()?;
            check_spelling(&alias, Spelling::Alias)?;
            if self.aliases.iter().any(|a| a.value() == alias.value()) {
                return Err(Error::new_spanned(
                    &alias,
                    format!("alias `{}` is given twice", alias.value()),
                ));
            }
            self.aliases.push(alias);
            Ok(true)
        } else if key == "hide" {
            if input.peek(Token![=]) {
                return Err(Error::new_spanned(
                    key,
                    "`hide` takes no value: write `hide`",
                ));
            }
            if self.hide.is_some() {
                return Err(Error::new_spanned(key, "`hide` is given twice"));
            }
            self.hide = Some(key.span());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reject an alias that is the command's own `name`: it would answer to
    /// the same word twice. `what` is `command` or `group`.
    pub(crate) fn check_name(&self, name: &str, what: &str) -> Result<()> {
        match self.aliases.iter().find(|a| a.value() == name) {
            Some(a) => Err(Error::new_spanned(
                a,
                format!("alias `{name}` is this {what}'s own name: an alias is another name"),
            )),
            None => Ok(()),
        }
    }
}

/// Which key a spelling was given under — what [`check_spelling`]'s messages
/// call it.
#[derive(Clone, Copy)]
pub(crate) enum Spelling {
    /// `name = "…"`.
    Name,
    /// `alias = "…"`.
    Alias,
}

impl Spelling {
    /// The key's noun, with its article: `a name`, `an alias`.
    fn label(self) -> &'static str {
        match self {
            Spelling::Name => "a name",
            Spelling::Alias => "an alias",
        }
    }
}

/// The checks one spelling — a `name` or an `alias` — passes on its own: it is
/// a non-empty single word that does not look like a flag.
pub(crate) fn check_spelling(lit: &LitStr, what: Spelling) -> Result<()> {
    let value = lit.value();
    let label = what.label();
    let problem = if value.is_empty() {
        Some(format!("{label} cannot be empty"))
    } else if value.chars().any(char::is_whitespace) {
        Some(format!("{label} is one word: it cannot contain whitespace"))
    } else if value.starts_with('-') {
        Some(format!(
            "{label} is a command name, not a flag: it cannot start with `-`"
        ))
    } else {
        None
    };
    match problem {
        Some(p) => Err(Error::new_spanned(lit, p)),
        None => Ok(()),
    }
}

/// Parse an attribute's `key = value, key, …` list.
///
/// The shared keys (`alias`, `hide`) are read into the returned
/// [`CommandMeta`]; every other key is offered to `own`, which reads it and
/// says `true`, or says `false` and the key is rejected with `unknown`.
pub(crate) fn parse_keys(
    input: ParseStream,
    unknown: &str,
    mut own: impl FnMut(&Ident, ParseStream) -> Result<bool>,
) -> Result<CommandMeta> {
    let mut meta = CommandMeta::default();
    while !input.is_empty() {
        let key = input
            .call(Ident::parse_any)
            .map_err(|e| Error::new(e.span(), unknown))?;
        if !(meta.read(&key, input)? || own(&key, input)?) {
            return Err(Error::new_spanned(&key, unknown));
        }
        if input.is_empty() {
            break;
        }
        let _: Token![,] = input.parse().map_err(|e| Error::new(e.span(), unknown))?;
    }
    Ok(meta)
}

/// `EventRef` -> `event_ref`: the name `#[derive(Bpaf)]` gives a type's parser
/// constructor.
pub(crate) fn snake_case(ident: &Ident) -> Ident {
    let name = ident.to_string();
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in chars.iter().copied().enumerate() {
        if !c.is_uppercase() {
            out.push(c);
            continue;
        }
        let after_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_numeric());
        let before_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
        if i > 0 && (after_lower || before_lower) {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    ident_or_raw(&out, ident.span())
}

/// Build an ident, falling back to a raw ident when the snake_case name is a
/// keyword (`struct Move` -> `r#move`).
fn ident_or_raw(name: &str, span: Span) -> Ident {
    match syn::parse_str::<Ident>(name) {
        Ok(mut id) => {
            id.set_span(span);
            id
        }
        Err(_) => Ident::new_raw(name, span),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::ItemFn;

    /// snake_case a type name the way `#[derive(Bpaf)]` does.
    fn snake(name: &str) -> String {
        snake_case(&Ident::new(name, Span::call_site())).to_string()
    }

    #[test]
    fn snake_case_splits_words() {
        assert_eq!(snake("EventRef"), "event_ref");
        assert_eq!(snake("Title"), "title");
        assert_eq!(snake("AddAdminArgs"), "add_admin_args");
    }

    #[test]
    fn snake_case_keeps_acronyms_together() {
        assert_eq!(snake("HTTPServer"), "http_server");
        assert_eq!(snake("URL"), "url");
    }

    #[test]
    fn snake_case_handles_digits_and_existing_underscores() {
        assert_eq!(snake("Event2Ref"), "event2_ref");
        assert_eq!(snake("event_ref"), "event_ref");
    }

    #[test]
    fn snake_case_escapes_a_keyword() {
        assert_eq!(snake("Move"), "r#move");
    }

    /// The doc-comment split, driven through a real function item.
    fn parts_of(src: &str) -> DocParts {
        let f: ItemFn = syn::parse_str(src).expect("parses");
        read_doc(&f.attrs, &f.sig, COMMAND_NEEDS_DOC).expect("has a doc comment")
    }

    /// Help and details only — the two slots that existed before the footer.
    fn doc_of(src: &str) -> (String, Option<String>) {
        let p = parts_of(src);
        (p.help, p.details)
    }

    /// The footer of `src`'s doc comment.
    fn footer_of_src(src: &str) -> Option<String> {
        parts_of(src).footer
    }

    #[test]
    fn the_first_line_is_the_help() {
        assert_eq!(
            doc_of("/// create an event\nfn create() {}"),
            ("create an event".to_string(), None)
        );
    }

    #[test]
    fn the_rest_is_the_details() {
        let (help, details) = doc_of(
            "/// create an event\n\
             ///\n\
             /// The directory is created.\n\
             ///\n\
             /// Twice.\n\
             fn create() {}",
        );
        assert_eq!(help, "create an event");
        assert_eq!(
            details.as_deref(),
            Some("The directory is created.\n\nTwice.")
        );
    }

    /// The body every examples test below is expected to produce.
    const MDMA: &str =
        "Examples:\n  mdma search --artist Bowie\n  mdma search --added=-7 --genre jazz";

    #[test]
    fn a_heading_with_a_sh_fenced_body_is_the_footer() {
        let p = parts_of(
            "/// search the library\n\
             ///\n\
             /// # Examples\n\
             ///\n\
             /// ```sh\n\
             /// mdma search --artist Bowie\n\
             /// mdma search --added=-7 --genre jazz\n\
             /// ```\n\
             fn search() {}",
        );
        assert_eq!(p.footer.as_deref(), Some(MDMA));
        assert_eq!(p.details, None);
    }

    #[test]
    fn a_terminal_style_heading_with_an_indented_body_is_the_footer() {
        assert_eq!(
            footer_of_src(
                "/// search the library\n\
                 ///\n\
                 /// Examples:\n\
                 ///     mdma search --artist Bowie\n\
                 ///     mdma search --added=-7 --genre jazz\n\
                 fn search() {}",
            )
            .as_deref(),
            Some(MDMA)
        );
    }

    #[test]
    fn the_body_is_dedented_then_indented_by_two_keeping_relative_indent() {
        assert_eq!(
            footer_of_src(
                "/// search the library\n\
                 /// # Examples\n\
                 ///         mdma search \\\n\
                 ///             --artist Bowie\n\
                 fn search() {}",
            )
            .as_deref(),
            Some("Examples:\n  mdma search \\\n      --artist Bowie")
        );
    }

    #[test]
    fn interior_blank_lines_are_kept_and_outer_ones_dropped() {
        assert_eq!(
            footer_of_src(
                "/// search the library\n\
                 /// # Examples\n\
                 ///\n\
                 ///\n\
                 /// mdma search --artist Bowie\n\
                 ///\n\
                 /// mdma search --added=-7 --genre jazz\n\
                 ///\n\
                 ///\n\
                 fn search() {}",
            )
            .as_deref(),
            Some(
                "Examples:\n  mdma search --artist Bowie\n\n  mdma search --added=-7 --genre jazz"
            )
        );
    }

    #[test]
    fn a_heading_with_nothing_under_it_is_rejected() {
        for src in [
            "/// search the library\n/// # Examples\nfn search() {}",
            "/// search the library\n/// Examples:\n///\n/// ```sh\n/// ```\nfn search() {}",
        ] {
            let f: ItemFn = syn::parse_str(src).expect("parses");
            let e = read_doc(&f.attrs, &f.sig, COMMAND_NEEDS_DOC).expect_err("empty examples");
            assert_eq!(e.to_string(), EXAMPLES_EMPTY);
        }
    }

    #[test]
    fn without_a_heading_there_is_no_footer_and_the_details_are_unchanged() {
        let p = parts_of(
            "/// search the library\n\
             ///\n\
             /// Searches.\n\
             ///\n\
             ///     mdma search --artist Bowie\n\
             fn search() {}",
        );
        assert_eq!(p.footer, None);
        assert_eq!(
            p.details.as_deref(),
            Some("Searches.\n\nmdma search --artist Bowie")
        );
    }

    #[test]
    fn the_details_stop_at_the_heading() {
        let p = parts_of(
            "/// search the library\n\
             ///\n\
             /// Searches by artist.\n\
             ///\n\
             /// # Examples\n\
             ///\n\
             /// ```sh\n\
             /// mdma search --artist Bowie\n\
             /// mdma search --added=-7 --genre jazz\n\
             /// ```\n\
             fn search() {}",
        );
        assert_eq!(p.details.as_deref(), Some("Searches by artist."));
        assert_eq!(p.footer.as_deref(), Some(MDMA));
    }

    #[test]
    fn a_second_level_heading_is_not_the_examples_heading() {
        let p = parts_of(
            "/// search the library\n\
             ///\n\
             /// ## Examples\n\
             ///\n\
             /// mdma search --artist Bowie\n\
             fn search() {}",
        );
        assert_eq!(p.footer, None);
        assert_eq!(
            p.details.as_deref(),
            Some("## Examples\n\nmdma search --artist Bowie")
        );
    }

    // --- the shared attribute keys --------------------------------------------

    /// `parse_keys` over `src`, with no keys of the caller's own.
    fn keys(src: &str) -> Result<CommandMeta> {
        use syn::parse::Parser;
        (|input: ParseStream| parse_keys(input, "unknown", |_, _| Ok(false))).parse_str(src)
    }

    /// The error `parse_keys` raises for `src`.
    fn keys_rejection(src: &str) -> String {
        match keys(src) {
            Ok(_) => panic!("`{src}` was accepted"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn aliases_repeat_in_source_order_and_hide_is_a_bare_word() {
        let m = keys("alias = \"ls\", hide, alias = \"l\"").expect("parses");
        let aliases: Vec<String> = m.aliases.iter().map(LitStr::value).collect();
        assert_eq!(aliases, ["ls", "l"]);
        assert!(m.hide.is_some());
        let m = keys("").expect("parses");
        assert!(m.aliases.is_empty() && m.hide.is_none());
    }

    #[test]
    fn each_bad_key_is_rejected() {
        assert_eq!(keys_rejection("help = \"x\""), "unknown");
        assert_eq!(keys_rejection("\"x\""), "unknown");
        assert_eq!(keys_rejection("hide, hide"), "`hide` is given twice");
        assert_eq!(
            keys_rejection("hide = true"),
            "`hide` takes no value: write `hide`"
        );
        assert_eq!(keys_rejection("alias = \"\""), "an alias cannot be empty");
        assert_eq!(
            keys_rejection("alias = \"l s\""),
            "an alias is one word: it cannot contain whitespace"
        );
        assert_eq!(
            keys_rejection("alias = \"-l\""),
            "an alias is a command name, not a flag: it cannot start with `-`"
        );
        assert_eq!(
            keys_rejection("alias = \"ls\", alias = \"ls\""),
            "alias `ls` is given twice"
        );
    }

    #[test]
    fn an_alias_equal_to_the_name_is_rejected() {
        let m = keys("alias = \"list\"").expect("parses");
        assert_eq!(
            m.check_name("list", "command")
                .expect_err("same word")
                .to_string(),
            "alias `list` is this command's own name: an alias is another name"
        );
        assert!(m.check_name("show", "command").is_ok());
    }

    #[test]
    fn an_item_without_a_doc_comment_is_rejected() {
        let f: ItemFn = syn::parse_str("fn create() {}").expect("parses");
        let e = read_doc(&f.attrs, &f.sig, COMMAND_NEEDS_DOC).expect_err("no doc comment");
        assert_eq!(e.to_string(), COMMAND_NEEDS_DOC);
    }

    #[test]
    fn an_all_blank_doc_comment_is_rejected_with_the_callers_message() {
        // A comment that is *present* but has no text is the same failure: there
        // is no first line to be the help.
        let f: ItemFn = syn::parse_str("///\n///\nfn create() {}").expect("parses");
        let e = read_doc(&f.attrs, &f.sig, GROUP_NEEDS_DOC).expect_err("blank doc comment");
        assert_eq!(e.to_string(), GROUP_NEEDS_DOC);
    }

    #[test]
    fn the_ident_becomes_a_dashed_name() {
        let id = Ident::new("add_admin", Span::call_site());
        assert_eq!(dashed(&id), "add-admin");
        assert_eq!(handle_ident(&id).to_string(), "__hicli_add_admin");
    }

    #[test]
    fn a_raw_ident_loses_its_escape() {
        let id = Ident::new_raw("move", Span::call_site());
        assert_eq!(dashed(&id), "move");
        assert_eq!(handle_ident(&id).to_string(), "__hicli_move");
    }

    #[test]
    fn cfg_attributes_are_the_ones_copied_onto_a_handle() {
        let f: ItemFn = syn::parse_str(
            "/// only here\n#[cfg(unix)]\n#[cfg_attr(test, allow(unused))]\n#[inline]\nfn go() {}",
        )
        .expect("parses");
        let copied: Vec<String> = cfg_attrs(&f.attrs)
            .iter()
            .map(|a| a.path().get_ident().expect("a simple path").to_string())
            .collect();
        assert_eq!(copied, ["cfg", "cfg_attr"]);
    }
}
