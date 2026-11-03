//! `commands![…]`: a list of registrations into a `Vec<Command<Ctx>>`.
//!
//! One rule decides everything: **a bare path names a handle**. `create`
//! becomes `__hicli_create()`, `audience::define` becomes
//! `audience::__hicli_define()` — the hidden sibling `#[command]` and
//! `#[group]` write next to the item they are named after. Anything that is
//! not a bare path is an expression, and goes through `hicli::IntoCommand`
//! as it always did.
//!
//! This is why it is a proc macro and not `macro_rules!`: rewriting `create`
//! into `__hicli_create` is an ident rewrite, which a declarative macro
//! cannot do.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Error, Expr, ExprPath, Path, PathArguments, Result, Token};

/// The message for a list element that is not an expression at all.
const NOT_AN_EXPRESSION: &str =
    "commands! takes a comma-separated list: a bare path (`event`, `audience::define`) names a \
     #[command]/#[group] handle; anything else must be an expression producing a Command";

/// Expand the contents of `commands![…]`.
pub(crate) fn expand(input: TokenStream) -> TokenStream {
    match list(input) {
        Ok(tokens) => tokens,
        Err(e) => e.to_compile_error(),
    }
}

/// The list, or the one diagnostic that explains what a list is.
fn list(input: TokenStream) -> Result<TokenStream> {
    let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
    let items = parser
        .parse2(input)
        // syn's own "expected expression" names the token but not the rule it
        // broke, and this is the one rule of the macro.
        .map_err(|e| Error::new(e.span(), NOT_AN_EXPRESSION))?;

    let elements: Vec<TokenStream> = items.into_iter().map(element).collect();
    Ok(if elements.is_empty() {
        // `vec![]` cannot infer its element type here; `Vec::new()` at least
        // fails at the binding, where the type is written.
        quote!(::std::vec::Vec::new())
    } else {
        quote!(::std::vec![#(#elements),*])
    })
}

/// One element: a handle call, or a command expression.
///
/// The parentheses of `(c)` — the documented way to pass a local holding a
/// `Command` — are dropped **after** the decision they exist to make, not
/// before it: they are what says "this is a value, not a name", and once that
/// has been read they are only `unused_parens` in the consumer's build.
fn element(expr: Expr) -> TokenStream {
    let expr = peel(expr);
    match handle_path(&expr) {
        Some(handle) => quote!(#handle()),
        None => {
            let expr = unwrap_parens(expr);
            quote!(::hicli::IntoCommand::into_command(#expr))
        }
    }
}

/// `(c)` -> `c`, once the element is known not to be a handle.
fn unwrap_parens(expr: Expr) -> Expr {
    match expr {
        Expr::Paren(p) if p.attrs.is_empty() => *p.expr,
        other => other,
    }
}

/// Strip the invisible groups a `macro_rules!` forwarder wraps a `$x:expr` in.
///
/// Without this, an element forwarded through a declarative macro arrives as
/// `Expr::Group(create)` and would be read as an expression, not a bare path —
/// the bug that makes `app!` forward token trees.
fn peel(mut expr: Expr) -> Expr {
    while let Expr::Group(g) = expr {
        expr = *g.expr;
    }
    expr
}

/// The handle path a bare path denotes, if this expression is one.
///
/// Everything but the last segment is kept verbatim — spans included, so a
/// wrong path is reported where it is written — and the last ident is replaced
/// by `__hicli_<ident>`, spanned at the ident it came from, which is
/// [`crate::shared::handle_ident`]'s one spelling of the rule.
fn handle_path(expr: &Expr) -> Option<Path> {
    let Expr::Path(ExprPath {
        attrs,
        qself: None,
        path,
    }) = expr
    else {
        return None;
    };
    if !attrs.is_empty() {
        return None;
    }
    // `create::<Ctx>` is a turbofish, not a name: leave it to `IntoCommand`.
    if !matches!(path.segments.last()?.arguments, PathArguments::None) {
        return None;
    }
    let mut handle = path.clone();
    let last = handle.segments.last_mut()?;
    last.ident = crate::shared::handle_ident(&last.ident);
    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::{Delimiter, Group};
    use quote::ToTokens;

    /// Expand `src` as the contents of `commands![…]`.
    fn expansion(src: &str) -> String {
        expand(syn::parse_str(src).expect("lexes")).to_string()
    }

    #[test]
    fn a_bare_ident_is_a_handle_call() {
        assert_eq!(
            expansion("create"),
            quote!(::std::vec![__hicli_create()]).to_string()
        );
    }

    #[test]
    fn only_the_last_segment_of_a_path_is_rewritten() {
        assert_eq!(
            expansion("audience::define"),
            quote!(::std::vec![audience::__hicli_define()]).to_string()
        );
    }

    #[test]
    fn a_leading_colon_survives() {
        assert_eq!(
            expansion("::other::cli::event"),
            quote!(::std::vec![::other::cli::__hicli_event()]).to_string()
        );
    }

    #[test]
    fn a_turbofish_is_an_expression() {
        // `version::<Ctx>` is a *call target*, not a name to rewrite.
        assert_eq!(
            expansion("version::<Ctx>"),
            quote!(::std::vec![::hicli::IntoCommand::into_command(
                version::<Ctx>
            )])
            .to_string()
        );
    }

    #[test]
    fn a_call_is_an_expression() {
        assert_eq!(
            expansion("hicli::external(ext())"),
            quote!(::std::vec![::hicli::IntoCommand::into_command(
                hicli::external(ext())
            )])
            .to_string()
        );
    }

    #[test]
    fn parentheses_are_how_a_local_is_passed() {
        // A bare `c` would name a handle; `(c)` is an expression, which is the
        // documented escape for a local holding a `Command`. The parentheses
        // have done their work by then, and are not passed on — `into_command((c))`
        // is `unused_parens` in the consumer's build.
        assert_eq!(
            expansion("(c)"),
            quote!(::std::vec![::hicli::IntoCommand::into_command(c)]).to_string()
        );
    }

    #[test]
    fn an_empty_list_is_an_empty_vec() {
        assert_eq!(
            expansion(""),
            quote!(::std::vec::Vec::new()).to_string(),
            "an empty list must not be `vec![]`, which cannot infer its type"
        );
    }

    #[test]
    fn a_trailing_comma_is_accepted() {
        assert_eq!(
            expansion("create, roster,"),
            quote!(::std::vec![__hicli_create(), __hicli_roster()]).to_string()
        );
    }

    #[test]
    fn an_invisible_group_is_peeled() {
        // What a `macro_rules!` forwarder produces when it matches `$c:expr`:
        // the path arrives wrapped in a `Delimiter::None` group. It still has
        // to read as a bare path.
        let inner = Group::new(Delimiter::None, quote!(create));
        let got = expand(inner.to_token_stream()).to_string();
        assert_eq!(got, quote!(::std::vec![__hicli_create()]).to_string());
    }

    #[test]
    fn an_item_is_not_an_expression() {
        let got = expansion("struct Foo;");
        assert!(got.contains(NOT_AN_EXPRESSION), "{got}");
    }
}
