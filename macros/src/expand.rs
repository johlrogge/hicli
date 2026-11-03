//! Writing what a `#[command]` becomes: the function, verbatim, and one handle.
//!
//! The annotated function is re-emitted **untouched** — same name, signature,
//! visibility, attributes (in their original order) and body — so it stays an
//! ordinary function that its module, its neighbours and its tests can call.
//! Beside it goes the **handle**: a hidden `fn __hicli_<ident>()` in the
//! value namespace returning a `::hicli::Command<Ctx>`, which is what a
//! registration by name (`commands![create]`) resolves to.
//!
//! Everything here is a call into `hicli`'s public API, so the expansion is
//! something the author could have written by hand — which is the point: there
//! is no hidden runtime behind `#[command]`.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Type};

use crate::parse::{CommandFn, CurriedParam, Slot};
use crate::shared::handle_doc;

/// Expand a validated [`CommandFn`] into the function and its handle.
pub(crate) fn expand(c: CommandFn) -> TokenStream {
    let CommandFn {
        name,
        help,
        details,
        footer,
        aliases,
        hide,
        ident,
        handle,
        cfg_attrs,
        default,
        curried,
        arg_tys,
        bindings,
        slots,
        ctx_ty,
        item,
    } = c;

    // With a `&T` parameter the handle names `T`; without one it is generic
    // over `Ctx` and the closure ignores the context it is handed.
    let (generics, ctx_ty, ctx_binding) = match ctx_ty {
        Some(t) => (quote!(), quote!(#t), quote!(ctx)),
        None => (quote!(<Ctx: 'static>), quote!(Ctx), quote!(_ctx)),
    };

    let (parser, args_pat) = parser_for(&arg_tys, &bindings);

    // The function is called with its arguments in the order it declared them,
    // and the context wherever it put it — a curried parameter included, which
    // is still a `Slot::Arg` under its own binding.
    let call_args = slots.iter().map(|s| match s {
        Slot::Arg(binding) => quote!(#binding),
        Slot::Ctx => quote!(ctx),
    });

    let metadata = metadata(
        details.as_deref(),
        footer.as_deref(),
        &aliases,
        hide,
        default,
    );
    let handle_doc = handle_doc(&ident);

    // A plain command parses all of its arguments; a curried one receives the
    // first from its group, between the parsed ones and the context.
    let (ret, call) = match curried {
        None => (
            quote!(::hicli::Command<#ctx_ty>),
            quote! {
                ::hicli::command(
                    #name,
                    #help,
                    #parser,
                    |#args_pat, #ctx_binding: & #ctx_ty| #ident(#(#call_args),*),
                )
            },
        ),
        Some(CurriedParam { ty, binding }) => (
            quote!(::hicli::Curried<#ty, #ctx_ty>),
            quote! {
                ::hicli::curried_command(
                    #name,
                    #help,
                    #parser,
                    |#args_pat, #binding: #ty, #ctx_binding: & #ctx_ty| #ident(#(#call_args),*),
                )
            },
        ),
    };

    quote! {
        #item

        #[doc = #handle_doc]
        #[doc(hidden)]
        #[allow(unreachable_pub)]
        #(#cfg_attrs)*
        pub fn #handle #generics () -> #ret {
            #call
            #metadata
        }
    }
}

/// The builder calls after `hicli::command(…)` / `hicli::group(…)`, in their
/// fixed order: `.details(..)`, `.footer(..)`, one `.alias(..)` per alias in
/// source order, `.hide()`, and — last — `.by_default()` for a `default`
/// command. Shared with `#[group]`'s handle, so the two spell the same doc
/// comment and the same keys the same way.
pub(crate) fn metadata(
    details: Option<&str>,
    footer: Option<&str>,
    aliases: &[LitStr],
    hide: bool,
    default: bool,
) -> TokenStream {
    let details = details.map(|d| quote!(.details(#d)));
    let footer = footer.map(|f| quote!(.footer(#f)));
    let hide = hide.then(|| quote!(.hide()));
    let default = default.then(|| quote!(.by_default()));
    quote! {
        #details
        #footer
        #(.alias(#aliases))*
        #hide
        #default
    }
}

/// The argument parser and the pattern the generated closure binds it with.
///
/// Arity 0 is `pure(())`; every other arity is a block that binds one local per
/// parameter to `<T as Arg>::parser()` and composes them with `construct!`,
/// **always with a trailing comma**. The comma is load-bearing at arity 1:
/// `construct!(a0)` compiles to a parser *of a parser* and fails later with a
/// confusing `Clone` error, while `construct!(a0,)` yields a plain `Title`.
///
/// The locals are not an accident of style: `bpaf`'s `construct!` accepts only
/// a single-segment `ident()` call as an element, so `<T as Arg>::parser()`
/// cannot be inlined into it.
fn parser_for(arg_tys: &[Type], bindings: &[Ident]) -> (TokenStream, TokenStream) {
    match bindings {
        [] => (quote!(::hicli::bpaf::pure(())), quote!(())),
        [one] => (parser_block(arg_tys, bindings), quote!(#one)),
        many => (parser_block(arg_tys, bindings), quote!((#(#many),*))),
    }
}

/// The parser block for arity ≥ 1.
///
/// Each `<T as ::hicli::Arg>::parser()` is spanned at the **parameter's
/// type**, so a type that is not an argument type reports `Arg`'s
/// `on_unimplemented` message on `t: Title` in the signature the author wrote,
/// not somewhere inside a generated handle.
fn parser_block(arg_tys: &[Type], bindings: &[Ident]) -> TokenStream {
    let lets = bindings.iter().zip(arg_tys).map(|(binding, ty)| {
        let parser = quote_spanned!(ty.span()=> <#ty as ::hicli::Arg>::parser());
        quote!(let #binding = #parser;)
    });
    quote! {
        {
            #(#lets)*
            ::hicli::bpaf::construct!(#(#bindings),* ,)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{CommandArgs, CommandFn};
    use syn::ItemFn;

    /// Expand `item` (with `attr` as `#[command]`'s own arguments) to a token
    /// string.
    fn expansion(attr: &str, item: &str) -> String {
        let args: CommandArgs = syn::parse_str(attr).expect("the attribute parses");
        let item: ItemFn = syn::parse_str(item).expect("the function parses");
        expand(CommandFn::from_item(args, item).expect("is a command")).to_string()
    }

    #[test]
    fn an_arity_two_command_is_re_emitted_with_a_handle_beside_it() {
        // The reference expansion, asserted whole: everything the macro does is
        // visible in one place, and a change to any of it has to be written
        // down here first.
        let got = expansion(
            "alias = \"new\", hide, alias = \"mk\"",
            "/// create an event\n\
             ///\n\
             /// The event directory is created if it does not exist.\n\
             ///\n\
             /// # Examples\n\
             ///\n\
             /// ```sh\n\
             /// demo create ./summer \"Summer Party\"\n\
             /// ```\n\
             pub fn create(r: EventRef, t: Title, ctx: &Ctx) -> Outcome { work() }",
        );
        let want = quote! {
            #[doc = " create an event"]
            #[doc = ""]
            #[doc = " The event directory is created if it does not exist."]
            #[doc = ""]
            #[doc = " # Examples"]
            #[doc = ""]
            #[doc = " ```sh"]
            #[doc = " demo create ./summer \"Summer Party\""]
            #[doc = " ```"]
            pub fn create(r: EventRef, t: Title, ctx: &Ctx) -> Outcome { work() }

            #[doc = "`hicli` registration handle for [`create`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_create() -> ::hicli::Command<Ctx> {
                ::hicli::command(
                    "create",
                    "create an event",
                    {
                        let a0 = <EventRef as ::hicli::Arg>::parser();
                        let a1 = <Title as ::hicli::Arg>::parser();
                        ::hicli::bpaf::construct!(a0, a1,)
                    },
                    |(a0, a1), ctx: &Ctx| create(a0, a1, ctx),
                )
                .details("The event directory is created if it does not exist.")
                .footer("Examples:\n  demo create ./summer \"Summer Party\"")
                .alias("new")
                .alias("mk")
                .hide()
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_curried_arity_two_command_receives_its_first_parameter() {
        // The reference expansion for `curried`: the same handle name, a
        // `Curried<T, Ctx>` return, `curried_command`, and a parser of the
        // *remaining* parameters only. The closure takes the parsed value, the
        // curried one, then the context — and the function is still called in
        // its declared order.
        let got = expansion(
            "curried",
            "/// cancel a download\n\
             pub fn cancel(s: SourceName, d: DownloadId, ctx: &Ctx) -> Outcome { work() }",
        );
        let want = quote! {
            #[doc = " cancel a download"]
            pub fn cancel(s: SourceName, d: DownloadId, ctx: &Ctx) -> Outcome { work() }

            #[doc = "`hicli` registration handle for [`cancel`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_cancel() -> ::hicli::Curried<SourceName, Ctx> {
                ::hicli::curried_command(
                    "cancel",
                    "cancel a download",
                    {
                        let a1 = <DownloadId as ::hicli::Arg>::parser();
                        ::hicli::bpaf::construct!(a1,)
                    },
                    |a1, a0: SourceName, ctx: &Ctx| cancel(a0, a1, ctx),
                )
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_curried_command_with_three_parameters_parses_the_last_two() {
        let got = expansion(
            "curried, alias = \"mv\"",
            "/// move a download\n\
             fn shift(s: SourceName, d: DownloadId, t: Target, ctx: &Ctx) {}",
        );
        assert!(
            got.contains(
                "let a1 = < DownloadId as :: hicli :: Arg > :: parser () ; \
                 let a2 = < Target as :: hicli :: Arg > :: parser () ; \
                 :: hicli :: bpaf :: construct ! (a1 , a2 ,)"
            ),
            "{got}"
        );
        assert!(
            got.contains(
                "| (a1 , a2) , a0 : SourceName , ctx : & Ctx | shift (a0 , a1 , a2 , ctx)"
            ),
            "{got}"
        );
        assert!(got.ends_with(". alias (\"mv\") }"), "{got}");
    }

    #[test]
    fn a_curried_command_with_nothing_else_parses_nothing() {
        // Arity 1: the curried parameter is the only one, so the parser is
        // `pure(())` and the closure binds `()` for it.
        let got = expansion(
            "curried",
            "/// sync a source\nfn sync(s: SourceName, ctx: &Ctx) {}",
        );
        let want = quote! {
            #[doc = " sync a source"]
            fn sync(s: SourceName, ctx: &Ctx) {}

            #[doc = "`hicli` registration handle for [`sync`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_sync() -> ::hicli::Curried<SourceName, Ctx> {
                ::hicli::curried_command(
                    "sync",
                    "sync a source",
                    ::hicli::bpaf::pure(()),
                    |(), a0: SourceName, ctx: &Ctx| sync(a0, ctx),
                )
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_context_free_curried_command_is_generic_over_ctx() {
        let got = expansion("curried", "/// show a source\nfn show(s: SourceName) {}");
        assert!(
            got.contains(
                "pub fn __hicli_show < Ctx : 'static > () -> :: hicli :: Curried < SourceName , \
                 Ctx >"
            ),
            "{got}"
        );
        assert!(
            got.contains("| () , a0 : SourceName , _ctx : & Ctx | show (a0)"),
            "{got}"
        );
    }

    #[test]
    fn a_curried_command_keeps_the_contexts_declared_position() {
        let got = expansion(
            "curried",
            "/// cancel a download\nfn cancel(ctx: &Ctx, s: SourceName, d: DownloadId) {}",
        );
        assert!(
            got.contains("| a1 , a0 : SourceName , ctx : & Ctx | cancel (ctx , a0 , a1)"),
            "{got}"
        );
    }

    #[test]
    fn a_default_command_ends_in_by_default() {
        let got = expansion("default", "/// search the library\nfn search(q: Query) {}");
        assert!(
            got.ends_with("| a0 , _ctx : & Ctx | search (a0) ,) . by_default () }"),
            "{got}"
        );
        assert!(got.contains("-> :: hicli :: Command < Ctx >"), "{got}");
    }

    #[test]
    fn by_default_comes_after_every_other_call() {
        // `default` refuses `alias` and `hide` and a multi-line doc, so the
        // order is pinned on `metadata` itself.
        let aliases: Vec<LitStr> = vec![syn::parse_str("\"s\"").expect("a literal")];
        let got = metadata(Some("d"), Some("f"), &aliases, true, true).to_string();
        assert_eq!(
            got,
            ". details (\"d\") . footer (\"f\") . alias (\"s\") . hide () . by_default ()"
        );
        assert_eq!(metadata(None, None, &[], false, false).to_string(), "");
    }

    #[test]
    fn a_context_free_arity_zero_command_is_generic_and_parses_nothing() {
        let got = expansion("", "/// print the version\nfn version() { print() }");
        let want = quote! {
            #[doc = " print the version"]
            fn version() { print() }

            #[doc = "`hicli` registration handle for [`version`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_version<Ctx: 'static>() -> ::hicli::Command<Ctx> {
                ::hicli::command(
                    "version",
                    "print the version",
                    ::hicli::bpaf::pure(()),
                    |(), _ctx: &Ctx| version(),
                )
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn arity_one_keeps_the_trailing_comma_and_binds_one_name() {
        let got = expansion(
            "name = \"add-admin\"",
            "/// add an administrator\n#[allow(unused)]\nfn add_admin(t: Title, ctx: &Ctx) {}",
        );
        // The parameter's type is resolved through `Arg`, bound to a local …
        assert!(
            got.contains("let a0 = < Title as :: hicli :: Arg > :: parser ()"),
            "arity 1 lost its `Arg::parser()` binding:\n{got}"
        );
        // … and composed with the trailing comma, which is load-bearing:
        // `construct!(a0)` would yield a parser-of-parser, not a `Title`.
        assert!(
            got.contains("construct ! (a0 ,)"),
            "arity 1 lost its trailing comma:\n{got}"
        );
        assert!(got.contains("| a0 , ctx : & Ctx |"), "{got}");
    }

    #[test]
    fn the_name_attribute_renames_the_command_and_not_the_handle() {
        // `#[command(name = …)]` is about the command line; the handle's ident
        // is always derived from the fn's, or a registration by name could not
        // find it.
        let got = expansion(
            "name = \"add-admin\"",
            "/// add an administrator\nfn add_admin(t: Title, ctx: &Ctx) {}",
        );
        assert!(got.contains("\"add-admin\""), "{got}");
        assert!(got.contains("pub fn __hicli_add_admin"), "{got}");
    }

    #[test]
    fn the_context_keeps_its_declared_position() {
        let got = expansion(
            "",
            "/// take the context first\nfn go(ctx: &Ctx, t: Title) {}",
        );
        assert!(got.contains("go (ctx , a0)"), "{got}");
    }

    #[test]
    fn a_cfg_is_copied_onto_the_handle() {
        // The handle has to disappear with the function it registers, or a
        // `#[cfg]`-ed-out command leaves a handle calling nothing.
        let got = expansion("", "/// only here\n#[cfg(unix)]\nfn only(ctx: &Ctx) {}");
        assert!(got.contains("# [cfg (unix)] fn only"), "{got}");
        assert!(got.contains("# [cfg (unix)] pub fn __hicli_only"), "{got}");
    }

    #[test]
    fn a_raw_ident_is_unescaped_in_the_name_and_the_handle() {
        let got = expansion("", "/// move a thing\nfn r#move(ctx: &Ctx) {}");
        assert!(
            got.contains("\"move\""),
            "the command name kept `r#`:\n{got}"
        );
        assert!(got.contains("pub fn __hicli_move"), "{got}");
        // … and the call still names the function as it is written.
        assert!(got.contains("r#move (ctx)"), "{got}");
    }

    #[test]
    fn the_function_keeps_its_attributes_in_their_original_order() {
        let got = expansion(
            "",
            "#[allow(unused)]\n/// help\n#[inline]\nfn go(ctx: &Ctx) {}",
        );
        assert!(
            got.starts_with("# [allow (unused)] # [doc = \" help\"] # [inline] fn go"),
            "{got}"
        );
    }
}
