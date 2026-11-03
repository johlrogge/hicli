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
use syn::{Attribute, Error, Expr, ExprLit, Ident, Lit, Meta, Result};

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
/// `group_mod!` synthesises one for every declaration it resolved itself, so
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
fn doc_text(attr: &Attribute) -> Option<String> {
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

/// Split the doc comment into the one-line help and the rest.
///
/// `attrs` is the item's whole attribute list, in source order: the doc
/// attributes are picked out of it and everything else ignored, because the
/// item is re-emitted with its attributes exactly as written.
///
/// Each line is trimmed (a `///` comment's text starts with a space), leading
/// blank lines are dropped, and the blank separator between the first line and
/// the rest disappears with them. A comment that is missing *or* entirely blank
/// fails with `missing`, spanned at `at` — the token the author has to put a
/// doc comment on.
pub(crate) fn read_doc<T: ToTokens>(
    attrs: &[Attribute],
    at: &T,
    missing: &str,
) -> Result<(String, Option<String>)> {
    let lines = attrs
        .iter()
        .filter_map(doc_text)
        .map(|l| l.trim().to_string());
    let mut lines = lines.skip_while(String::is_empty);
    let Some(help) = lines.next() else {
        return Err(Error::new_spanned(at, missing));
    };
    let rest: Vec<String> = lines.skip_while(String::is_empty).collect();
    let details = rest.join("\n").trim_end().to_string();
    Ok((help, (!details.is_empty()).then_some(details)))
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
    fn doc_of(src: &str) -> (String, Option<String>) {
        let f: ItemFn = syn::parse_str(src).expect("parses");
        read_doc(&f.attrs, &f.sig, COMMAND_NEEDS_DOC).expect("has a doc comment")
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
