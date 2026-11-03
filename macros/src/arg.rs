//! `#[derive(Arg)]`: the impl that says an argument type parses through its own
//! `bpaf` constructor.
//!
//! The whole derive is one sentence — *`Id`'s parser is `id()`* — plus an
//! optional completer wrapped around it. The constructor is the one
//! `#[derive(Bpaf)]` writes, named by snake-casing the type's ident — the rule
//! that used to live in `#[command]`, now that `#[command]` asks the type through
//! `Arg` instead of guessing a name at the call site. The call is **spanned at
//! the type's ident**: a type that derives
//! `Arg` without `Bpaf` gets rustc's own *"cannot find function `id`"* pointing
//! at `Id`, which is a better message than any wrapper could produce.
//!
//! The completion helpers are emitted as **functions**
//! (`::hicli::complete_with(…)`), never as methods on the parser, so the
//! expansion needs no trait in scope and reads as what the author could have
//! written by hand.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Data, DeriveInput, Error, Ident, Meta, Path, PathArguments, Result, Token};

use crate::shared::snake_case;

/// The message for an `#[arg(…)]` argument that is not `complete = …` (A1).
const UNKNOWN_ARG: &str = "#[arg(...)] takes one argument: `complete = <fn>`, `complete = files` \
                           or `complete = dirs`";

/// The message for a completer on something that is not a single value (A2).
///
/// Both right answers are named, because which one fits depends on what the
/// type is *for* — and the reason is stated, because "it compiles but never
/// fires" is the failure this rejection exists to prevent.
const COMPLETE_NEEDS_ONE_FIELD: &str =
    "#[arg(complete = ...)] needs a single-field struct: a completer on the whole value is handed \
     the value that already parsed, so it cannot tell which field is being completed and never \
     fires while another field is mid-word\n\
     note: either make the value a newtype and complete that, or complete the field itself with \
     bpaf's own #[bpaf(complete(f))]";

/// The message for a second `#[arg(…)]` on the same type.
///
/// One attribute, one answer. Two used to let the last one win in silence,
/// which is a question — *which completer is this type using?* — that the
/// source could no longer answer.
const TWO_ATTRIBUTES: &str = "#[derive(Arg)] takes at most one `#[arg(...)]` attribute";

/// The message for `#[derive(Arg)]` on a union (A3).
const A_UNION: &str = "#[derive(Arg)] goes on a struct or an enum: an argument type is concrete, \
                       and there is no #[derive(Bpaf)] for a union to build a parser from";

/// The message for `#[derive(Arg)]` on a generic type (A4).
const GENERIC: &str = "#[derive(Arg)] cannot be generic: an argument type is concrete — \
                       `Arg::parser()` takes no parameters, so there is nothing to fix a type \
                       parameter with";

/// What a completer is, if there is one.
enum Completion {
    /// None asked for: the constructor, unwrapped.
    None,
    /// `complete = files` — the shell's own file completion.
    Files,
    /// `complete = dirs` — the shell's own directory completion.
    Dirs,
    /// `complete = <path>` — a function of the author's.
    With(Path),
}

/// What `#[arg(…)]` was given, and where it was written.
struct ArgArgs {
    /// The completer asked for.
    completion: Completion,
}

impl Parse for ArgArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Ok(ArgArgs {
                completion: Completion::None,
            });
        }
        let key: Ident = input
            .parse()
            .map_err(|e| Error::new(e.span(), UNKNOWN_ARG))?;
        if key != "complete" {
            return Err(Error::new_spanned(&key, UNKNOWN_ARG));
        }
        let _: Token![=] = input.parse()?;
        let path: Path = input.parse()?;
        if !input.is_empty() {
            return Err(input.error(UNKNOWN_ARG));
        }
        Ok(ArgArgs {
            completion: shorthand(path),
        })
    }
}

/// Read `files` / `dirs` as the shorthands they are, before reading them as
/// paths.
///
/// A bare single-segment word wins; `self::files` is a path, and is how a
/// completer function genuinely called `files` is named.
fn shorthand(path: Path) -> Completion {
    let bare = path.leading_colon.is_none() && path.segments.len() == 1;
    let seg = path.segments.first();
    match seg {
        Some(s) if bare && matches!(s.arguments, PathArguments::None) && s.ident == "files" => {
            Completion::Files
        }
        Some(s) if bare && matches!(s.arguments, PathArguments::None) && s.ident == "dirs" => {
            Completion::Dirs
        }
        _ => Completion::With(path),
    }
}

/// The `#[arg(…)]` on the type, if any: what it asked for, and the attribute it
/// asked on — which is the span a shape rejection points at.
fn read_attrs(attrs: &[Attribute]) -> Result<(Completion, Option<Attribute>)> {
    let mut found = (Completion::None, None);
    for attr in attrs.iter().filter(|a| a.path().is_ident("arg")) {
        match &attr.meta {
            // `#[arg]` on its own says nothing, and is allowed to: it is what a
            // half-written attribute looks like.
            Meta::Path(_) => {}
            Meta::List(list) => {
                if found.1.is_some() {
                    // Spanned at the *second* one: the first is the one that
                    // was already read, so this is the token to delete.
                    return Err(Error::new_spanned(attr, TWO_ATTRIBUTES));
                }
                let args: ArgArgs = syn::parse2(list.tokens.clone())?;
                found = (args.completion, Some(attr.clone()));
            }
            Meta::NameValue(nv) => return Err(Error::new_spanned(nv, UNKNOWN_ARG)),
        }
    }
    Ok(found)
}

/// Expand `#[derive(Arg)]`, or explain why this type cannot be an argument.
pub(crate) fn expand(input: DeriveInput) -> Result<TokenStream> {
    if let Data::Union(_) = &input.data {
        return Err(Error::new_spanned(&input.ident, A_UNION));
    }
    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(&input.generics, GENERIC));
    }
    if let Some(w) = &input.generics.where_clause {
        return Err(Error::new_spanned(w, GENERIC));
    }

    let (completion, at) = read_attrs(&input.attrs)?;
    if !matches!(completion, Completion::None) && !is_single_field(&input.data) {
        let at = at.expect("a completion was asked for by an attribute");
        return Err(Error::new_spanned(at, COMPLETE_NEEDS_ONE_FIELD));
    }

    let ty = &input.ident;
    // Spanned at the type's ident, so a missing `#[derive(Bpaf)]` is reported on
    // `Id` rather than somewhere inside a generated impl.
    let ctor = snake_case(ty);
    let parser = match completion {
        Completion::None => quote!(#ctor()),
        Completion::Files => quote!(::hicli::complete_files(#ctor())),
        Completion::Dirs => quote!(::hicli::complete_dirs(#ctor())),
        Completion::With(f) => quote!(::hicli::complete_with(#ctor(), #f)),
    };

    Ok(quote! {
        #[automatically_derived]
        impl ::hicli::Arg for #ty {
            fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                #parser
            }
        }
    })
}

/// Is this one value with one field — the only shape a whole-value completer
/// can speak for?
fn is_single_field(data: &Data) -> bool {
    match data {
        Data::Struct(s) => s.fields.len() == 1,
        // An enum's completer would have to speak for every variant at once.
        Data::Enum(_) | Data::Union(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expand `src` to a token string.
    fn expansion(src: &str) -> String {
        let input: DeriveInput = syn::parse_str(src).expect("the type parses");
        expand(input).expect("is an argument type").to_string()
    }

    /// The error `#[derive(Arg)]` raises for `src`.
    fn rejection(src: &str) -> String {
        let input: DeriveInput = syn::parse_str(src).expect("the type parses");
        expand(input)
            .expect_err("is not an argument type")
            .to_string()
    }

    #[test]
    fn a_newtype_parses_through_its_bpaf_constructor() {
        // The reference expansion, asserted whole: everything the derive does is
        // visible in one place.
        let got = expansion("/// an event id\npub struct Id(String);");
        let want = quote! {
            #[automatically_derived]
            impl ::hicli::Arg for Id {
                fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                    id()
                }
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_completer_wraps_the_constructor() {
        let got = expansion("#[arg(complete = complete_ids)]\npub struct Id(String);");
        let want = quote! {
            #[automatically_derived]
            impl ::hicli::Arg for Id {
                fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                    ::hicli::complete_with(id(), complete_ids)
                }
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_path_completer_is_kept_as_written() {
        let got = expansion("#[arg(complete = crate::ids::complete)]\npub struct Id(String);");
        assert!(
            got.contains(":: hicli :: complete_with (id () , crate :: ids :: complete)"),
            "{got}"
        );
    }

    #[test]
    fn the_shorthands_are_read_as_words_not_paths() {
        let dirs = expansion("#[arg(complete = dirs)]\npub struct Home(String);");
        assert!(
            dirs.contains(":: hicli :: complete_dirs (home ())"),
            "{dirs}"
        );
        let files = expansion("#[arg(complete = files)]\npub struct Input(String);");
        assert!(
            files.contains(":: hicli :: complete_files (input ())"),
            "{files}"
        );
        // … and `self::files` is a path again, which is how a completer function
        // of that name is named.
        let own = expansion("#[arg(complete = self::files)]\npub struct Input(String);");
        assert!(
            own.contains(":: hicli :: complete_with (input () , self :: files)"),
            "{own}"
        );
    }

    #[test]
    fn a_multi_field_struct_without_completion_is_the_same_shape() {
        let got = expansion("pub struct ShareOpts { a: bool, b: bool }");
        let want = quote! {
            #[automatically_derived]
            impl ::hicli::Arg for ShareOpts {
                fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                    share_opts()
                }
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn an_acronym_snake_cases_the_way_bpaf_does() {
        let got = expansion("pub struct HTTPServer(String);");
        assert!(got.contains("http_server ()"), "{got}");
    }

    #[test]
    fn a_bare_arg_attribute_says_nothing() {
        assert_eq!(
            expansion("#[arg]\npub struct Id(String);"),
            expansion("pub struct Id(String);")
        );
    }

    #[test]
    fn an_unknown_key_names_the_one_argument() {
        assert_eq!(
            rejection("#[arg(name = \"ID\")]\npub struct Id(String);"),
            UNKNOWN_ARG
        );
        assert_eq!(
            rejection("#[arg(\"ID\")]\npub struct Id(String);"),
            UNKNOWN_ARG
        );
        assert_eq!(
            rejection("#[arg = \"ID\"]\npub struct Id(String);"),
            UNKNOWN_ARG
        );
        assert_eq!(
            rejection("#[arg(complete = f, complete = g)]\npub struct Id(String);"),
            UNKNOWN_ARG
        );
    }

    #[test]
    fn a_second_arg_attribute_is_refused_rather_than_ignored() {
        // Two `#[arg(…)]` attributes used to let the last one win in silence,
        // which is the worst possible answer to "which completer did I get?".
        assert_eq!(
            rejection("#[arg(complete = f)]\n#[arg(complete = g)]\npub struct Id(String);"),
            TWO_ATTRIBUTES
        );
        // A bare `#[arg]` still says nothing, so it is not a second anything.
        assert_eq!(
            expansion("#[arg]\n#[arg(complete = f)]\npub struct Id(String);"),
            expansion("#[arg(complete = f)]\npub struct Id(String);")
        );
    }

    #[test]
    fn a_completer_on_a_multi_field_struct_names_both_right_answers() {
        assert_eq!(
            rejection("#[arg(complete = f)]\npub struct Ref { dir: String, name: String }"),
            COMPLETE_NEEDS_ONE_FIELD
        );
    }

    #[test]
    fn a_completer_on_an_enum_is_the_same_rejection() {
        assert_eq!(
            rejection("#[arg(complete = f)]\npub enum Mode { Fast, Slow }"),
            COMPLETE_NEEDS_ONE_FIELD
        );
    }

    #[test]
    fn an_enum_without_completion_is_fine() {
        let got = expansion("pub enum Mode { Fast, Slow }");
        assert!(got.contains("impl :: hicli :: Arg for Mode"), "{got}");
        assert!(got.contains("mode ()"), "{got}");
    }

    #[test]
    fn a_union_is_rejected() {
        assert_eq!(rejection("pub union Id { a: u8, b: u8 }"), A_UNION);
    }

    #[test]
    fn a_generic_type_is_rejected() {
        assert_eq!(rejection("pub struct Id<T>(T);"), GENERIC);
        assert_eq!(
            rejection("pub struct Id(String) where String: Clone;"),
            GENERIC
        );
    }
}
