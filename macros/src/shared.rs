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
    "a #[command] needs a doc comment: its first paragraph is the command's help";

/// The same rule for a `#[group]` (ADR-001, ADR-005 §3).
pub(crate) const GROUP_NEEDS_DOC: &str =
    "a #[group] needs a doc comment: its first paragraph is the group's help";

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

/// The message for a line under `# Arguments` that is neither an entry nor
/// an entry's indented continuation.
pub(crate) const ARGUMENTS_MALFORMED: &str = "each entry under `# Arguments` is `* `name` - help`";

/// The message for an `# Arguments` heading with no entries under it.
pub(crate) const ARGUMENTS_EMPTY: &str = "this # Arguments section is empty: give at least one \
                                          `* `name` - help`, or remove the heading";

/// The slots a doc comment fills (ADR-008, ADR-016).
#[derive(Debug)]
pub(crate) struct DocParts {
    /// The first paragraph, joined into one line: the help, and the heading
    /// of `--help`.
    pub(crate) help: String,
    /// The prose between the help and the first section heading — `bpaf`'s
    /// header, which reflows.
    pub(crate) details: Option<String>,
    /// The `# Arguments` section, if there is one.
    pub(crate) arguments: Option<Arguments>,
    /// The examples section, already laid out: `Examples:` and the body
    /// indented by two — rendered verbatim after the options.
    pub(crate) footer: Option<String>,
}

/// A command's `# Arguments` section: per-parameter help overrides.
#[derive(Debug)]
pub(crate) struct Arguments {
    /// The heading's doc attribute — where an error about the whole section
    /// points.
    pub(crate) heading: proc_macro2::TokenStream,
    /// The entries, in source order.
    pub(crate) entries: Vec<ArgEntry>,
}

/// One `* `name` - help` entry.
#[derive(Debug)]
pub(crate) struct ArgEntry {
    /// The parameter it names.
    pub(crate) name: String,
    /// Its help: the entry's text and its continuation lines, joined by a
    /// space.
    pub(crate) help: String,
    /// The doc attribute the entry starts on — where an error about it points.
    pub(crate) at: proc_macro2::TokenStream,
}

/// Read an entry's first line — `* `name` - help`, or with `-` as the bullet —
/// into its name and help.
fn entry_line(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    let t = t.strip_prefix('*').or_else(|| t.strip_prefix('-'))?;
    let t = t.trim_start().strip_prefix('`')?;
    let (name, rest) = t.split_once('`')?;
    let help = rest.trim_start().strip_prefix('-')?.trim();
    if name.trim().is_empty() || name.trim() != name || help.is_empty() {
        return None;
    }
    Some((name.to_string(), help.to_string()))
}

/// Read the lines under an `# Arguments` heading into entries.
///
/// An entry starts at a bullet line and continues on lines indented deeper
/// than its bullet, which are joined to it with a space; a blank line, the
/// next bullet or the end of the section ends it. Anything else is
/// [`ARGUMENTS_MALFORMED`].
fn arguments_of(heading: &Attribute, body: &[(&Attribute, String)]) -> Result<Arguments> {
    let mut entries: Vec<ArgEntry> = Vec::new();
    // The open entry's bullet indentation, while it can still continue.
    let mut open: Option<usize> = None;
    for (attr, line) in body {
        if line.trim().is_empty() {
            open = None;
            continue;
        }
        let indent = indent_of(line);
        if let Some((name, help)) = entry_line(line) {
            entries.push(ArgEntry {
                name,
                help,
                at: attr.to_token_stream(),
            });
            open = Some(indent);
            continue;
        }
        match (open, entries.last_mut()) {
            (Some(bullet), Some(entry)) if indent > bullet => {
                entry.help.push(' ');
                entry.help.push_str(line.trim());
            }
            _ => return Err(Error::new_spanned(attr, ARGUMENTS_MALFORMED)),
        }
    }
    if entries.is_empty() {
        return Err(Error::new_spanned(heading, ARGUMENTS_EMPTY));
    }
    Ok(Arguments {
        heading: heading.to_token_stream(),
        entries,
    })
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

/// Is this line the heading that starts the `# Arguments` section?
///
/// Trimmed, exactly `# Arguments` or `Arguments:`, like the examples heading.
pub(crate) fn is_arguments_heading(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "# Arguments" || trimmed == "Arguments:"
}

/// Does this line start a section — and so end the paragraph before it?
fn is_section_heading(line: &str) -> bool {
    is_examples_heading(line) || is_arguments_heading(line)
}

/// A doc comment's lines, one entry per line, each with the attribute it came
/// from (for spans).
///
/// A `#[doc = "a\nb"]` is several lines; `split` rather than `lines` so an
/// empty `///` is still one (blank) line. Lines are not trimmed: the examples
/// body keeps its indentation.
pub(crate) fn doc_lines(attrs: &[Attribute]) -> Vec<(&Attribute, String)> {
    attrs
        .iter()
        .filter_map(|a| doc_text(a).map(|t| (a, t)))
        .flat_map(|(a, t)| {
            t.split('\n')
                .map(|l| (a, l.trim_end_matches('\r').to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The first paragraph of a doc comment — the help, everywhere — and the index
/// of the first line after it; `None` if every line is blank.
///
/// The paragraph is the first non-blank line and the non-blank lines straight
/// after it, up to the first blank line or a section heading (`# Examples`,
/// `# Arguments` and their terminal spellings), each trimmed and joined with a
/// single space. The first line is always part of it, whatever it says.
pub(crate) fn first_paragraph<A>(lines: &[(A, String)]) -> Option<(String, usize)> {
    let first = lines.iter().position(|(_, l)| !l.trim().is_empty())?;
    let end = lines[first + 1..]
        .iter()
        .position(|(_, l)| l.trim().is_empty() || is_section_heading(l))
        .map_or(lines.len(), |n| first + 1 + n);
    let help = lines[first..end]
        .iter()
        .map(|(_, l)| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
    Some((help, end))
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

/// Split the doc comment into the help, the details and the examples footer.
///
/// `attrs` is the item's whole attribute list, in source order: the doc
/// attributes are picked out of it and everything else ignored, because the
/// item is re-emitted with its attributes exactly as written.
///
/// **Help:** the first paragraph — see [`first_paragraph`]. Consecutive lines
/// are joined with a space, so a help that runs onto a second line is still
/// one help; the details start after the first blank line. A comment that is
/// missing *or* entirely blank fails with `missing`, spanned at `at` — the
/// token the author has to put a doc comment on.
///
/// **Details:** each line after the help is trimmed (a `///` comment's text
/// starts with a space) and leading blank lines are dropped.
///
/// **Arguments:** a section headed by a line that is exactly `# Arguments` or
/// `Arguments:` also ends the details, and runs to the examples heading or
/// the end of the comment — see [`arguments_of`]. After the examples heading
/// it is not a heading: everything there is the examples body, verbatim.
///
/// **Footer:** the details stop at the first line after the help whose trimmed
/// text is exactly `# Examples` or `Examples:`; everything after it is the
/// examples body, read *without* trimming — see [`footer_of`]. A heading
/// straight after the help, with no blank line, ends the help.
pub(crate) fn read_doc<T: ToTokens>(
    attrs: &[Attribute],
    at: &T,
    missing: &str,
) -> Result<DocParts> {
    let lines = doc_lines(attrs);

    let Some((help, end)) = first_paragraph(&lines) else {
        return Err(Error::new_spanned(at, missing));
    };
    let rest = &lines[end..];

    let heading = rest.iter().position(|(_, l)| is_examples_heading(l));
    let before_examples = match heading {
        Some(h) => &rest[..h],
        None => rest,
    };
    let section = before_examples
        .iter()
        .position(|(_, l)| is_arguments_heading(l));
    let arguments = match section {
        Some(s) => Some(arguments_of(
            before_examples[s].0,
            &before_examples[s + 1..],
        )?),
        None => None,
    };
    let prose = match section {
        Some(s) => &before_examples[..s],
        None => before_examples,
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
        arguments,
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
    fn a_one_line_paragraph_is_the_help() {
        assert_eq!(
            doc_of("/// create an event\nfn create() {}"),
            ("create an event".to_string(), None)
        );
    }

    #[test]
    fn the_first_paragraph_is_the_help_joined_into_one_line() {
        // BREAKING (ADR-016): this second line used to be the details.
        assert_eq!(
            doc_of(
                "///   create an event  \n\
                 /// in the current directory\n\
                 fn create() {}"
            ),
            ("create an event in the current directory".to_string(), None)
        );
        let (help, details) = doc_of(
            "///\n\
             /// create an event\n\
             /// in the current directory\n\
             ///\n\
             /// The directory is created.\n\
             /// It must not exist.\n\
             fn create() {}",
        );
        assert_eq!(help, "create an event in the current directory");
        // The details keep their own line breaks; bpaf reflows them.
        assert_eq!(
            details.as_deref(),
            Some("The directory is created.\nIt must not exist.")
        );
    }

    #[test]
    fn a_heading_ends_the_first_paragraph() {
        let p = parts_of(
            "/// search the library\n\
             /// by text\n\
             /// # Examples\n\
             /// mdma search --artist Bowie\n\
             fn search() {}",
        );
        assert_eq!(p.help, "search the library by text");
        assert_eq!(p.details, None);
        assert_eq!(
            p.footer.as_deref(),
            Some("Examples:\n  mdma search --artist Bowie")
        );
        let p =
            parts_of("/// search the library\n/// Examples:\n///   mdma search\nfn search() {}");
        assert_eq!(p.help, "search the library");
        assert_eq!(p.footer.as_deref(), Some("Examples:\n  mdma search"));
    }

    /// `(name, help)` of each `# Arguments` entry in `src`.
    fn entries_of(src: &str) -> Vec<(String, String)> {
        parts_of(src)
            .arguments
            .expect("has an # Arguments section")
            .entries
            .into_iter()
            .map(|e| (e.name, e.help))
            .collect()
    }

    /// The error `read_doc` raises for `src`.
    fn doc_rejection(src: &str) -> String {
        let f: ItemFn = syn::parse_str(src).expect("parses");
        read_doc(&f.attrs, &f.sig, COMMAND_NEEDS_DOC)
            .expect_err("rejected")
            .to_string()
    }

    #[test]
    fn an_arguments_section_is_read_and_left_out_of_the_details() {
        let p = parts_of(
            "/// list tracks\n\
             ///\n\
             /// Prints one track per line.\n\
             ///\n\
             /// # Arguments\n\
             ///\n\
             /// * `by` - Order for\n\
             ///   this listing\n\
             ///\n\
             /// - `limit` - At most this many\n\
             /// * `r#type` -   What kind  \n\
             ///\n\
             /// # Examples\n\
             ///\n\
             /// playr list\n\
             fn list() {}",
        );
        assert_eq!(p.help, "list tracks");
        assert_eq!(p.details.as_deref(), Some("Prints one track per line."));
        assert_eq!(p.footer.as_deref(), Some("Examples:\n  playr list"));
        let entries: Vec<(String, String)> = p
            .arguments
            .expect("a section")
            .entries
            .into_iter()
            .map(|e| (e.name, e.help))
            .collect();
        assert_eq!(
            entries,
            [
                ("by".to_string(), "Order for this listing".to_string()),
                ("limit".to_string(), "At most this many".to_string()),
                ("r#type".to_string(), "What kind".to_string()),
            ]
        );
    }

    #[test]
    fn the_terminal_heading_and_a_heading_ending_the_help_work_too() {
        assert_eq!(
            entries_of("/// list\n/// Arguments:\n/// * `by` - x\nfn list() {}"),
            [("by".to_string(), "x".to_string())]
        );
        let p = parts_of("/// list tracks\n/// # Arguments\n/// * `by` - x\nfn list() {}");
        assert_eq!(p.help, "list tracks");
        assert_eq!(p.details, None);
        assert_eq!(p.footer, None);
    }

    #[test]
    fn an_arguments_heading_after_examples_is_examples_body() {
        let p = parts_of(
            "/// list\n\
             ///\n\
             /// # Examples\n\
             /// playr list\n\
             /// # Arguments\n\
             /// * `by` - x\n\
             fn list() {}",
        );
        assert!(p.arguments.is_none());
        assert_eq!(
            p.footer.as_deref(),
            Some("Examples:\n  playr list\n  # Arguments\n  * `by` - x")
        );
    }

    #[test]
    fn a_line_that_is_not_an_entry_is_rejected() {
        for bad in [
            // prose under the heading
            "/// list\n/// # Arguments\n/// Some prose.\nfn list() {}",
            // a continuation after a blank line has nothing to continue
            "/// list\n/// # Arguments\n/// * `by` - x\n///\n///   more\nfn list() {}",
            // a continuation not indented past its bullet
            "/// list\n/// # Arguments\n/// * `by` - x\n/// more\nfn list() {}",
            // no help
            "/// list\n/// # Arguments\n/// * `by` -\nfn list() {}",
            "/// list\n/// # Arguments\n/// * `by` -   \nfn list() {}",
            // no dash, no backticks, an empty name
            "/// list\n/// # Arguments\n/// * `by` help\nfn list() {}",
            "/// list\n/// # Arguments\n/// * by - help\nfn list() {}",
            "/// list\n/// # Arguments\n/// * `` - help\nfn list() {}",
            // a second section heading
            "/// list\n/// # Arguments\n/// * `by` - x\n/// # Arguments\nfn list() {}",
        ] {
            assert_eq!(doc_rejection(bad), ARGUMENTS_MALFORMED, "{bad}");
        }
    }

    #[test]
    fn an_empty_arguments_section_is_rejected() {
        for bad in [
            "/// list\n///\n/// # Arguments\nfn list() {}",
            "/// list\n///\n/// # Arguments\n///\n///\n/// # Examples\n/// x\nfn list() {}",
        ] {
            assert_eq!(doc_rejection(bad), ARGUMENTS_EMPTY, "{bad}");
        }
    }

    #[test]
    fn first_paragraph_reads_any_lines() {
        let lines = |ls: &[&str]| ls.iter().map(|l| ((), l.to_string())).collect::<Vec<_>>();
        assert_eq!(first_paragraph(&lines(&[])), None);
        assert_eq!(first_paragraph(&lines(&["", "  "])), None);
        assert_eq!(
            first_paragraph(&lines(&["", " a ", "b", "", "c"])),
            Some(("a b".to_string(), 3))
        );
        assert_eq!(
            first_paragraph(&lines(&["a", "# Arguments", "* `x` - y"])),
            Some(("a".to_string(), 1))
        );
        assert_eq!(
            first_paragraph(&lines(&["a", "b"])),
            Some(("a b".to_string(), 2))
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
