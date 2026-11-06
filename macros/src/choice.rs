//! `#[derive(Choice)]`: an enum whose variants are a fixed set of words.
//!
//! The derive writes three impls, all read off the variants: `hicli::Choice`
//! (the variants, each one's name and its description), `FromStr` (an exact,
//! case-sensitive match on the names, `hicli::UnknownChoice` otherwise) and
//! `Display` (the name). A variant's name is its ident in kebab-case —
//! `PlayCount` → `play-count` — unless `#[choice(name = "…")]` says otherwise;
//! its description is the first line of its doc comment.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::{Attribute, Data, DeriveInput, Error, Fields, Ident, LitStr, Meta, Result, Token};

use crate::shared::{doc_text, snake_case};

/// C1: the derive on a struct or a union.
const NOT_AN_ENUM: &str = "#[derive(Choice)] goes on an enum whose variants have no fields";

/// C2: a variant with fields.
const VARIANT_WITH_FIELDS: &str =
    "a choice variant has no fields: it is one of a fixed set of words";

/// C3: an enum with no variants.
const NO_VARIANTS: &str = "#[derive(Choice)] needs at least one variant";

/// C4: generics or a where clause — `#[derive(Arg)]`'s `GENERIC`, for a choice.
const GENERIC: &str = "#[derive(Choice)] cannot be generic: a choice is concrete — \
                       `Choice::VARIANTS` is a constant list of values, so there is nothing to \
                       fix a type parameter with";

/// C6: a `name` that is not one word a user can type as a value.
const BAD_NAME: &str = "a choice name is one non-empty word that does not start with `-`";

/// C7: anything in `#[choice(…)]` but one `name = "…"`.
const ONE_KEY: &str = "#[choice(...)] takes one key: `name = \"…\"`";

/// C8: `#[choice]` on the enum rather than on a variant.
const ON_THE_ENUM: &str = "#[choice(name = …)] goes on a variant";

/// C5: two variants spelled the same.
fn duplicate(name: &str) -> String {
    format!("two choices are both spelled `{name}`")
}

/// One variant, read: its ident, the word that spells it, where that word came
/// from (for C5's span) and its description.
struct Variant {
    ident: Ident,
    name: String,
    at: proc_macro2::Span,
    description: Option<String>,
}

/// The `name = "…"` in a variant's `#[choice(…)]`, if it has one.
///
/// Two `#[choice]` attributes are two keys, so C7 as well.
fn read_name(attrs: &[Attribute]) -> Result<Option<LitStr>> {
    let mut found: Option<LitStr> = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("choice")) {
        let Meta::List(list) = &attr.meta else {
            return Err(Error::new_spanned(attr, ONE_KEY));
        };
        if found.is_some() {
            return Err(Error::new_spanned(attr, ONE_KEY));
        }
        let lit = list.parse_args_with(|input: syn::parse::ParseStream| {
            let key = input
                .call(Ident::parse_any)
                .map_err(|e| Error::new(e.span(), ONE_KEY))?;
            if key != "name" {
                return Err(Error::new_spanned(&key, ONE_KEY));
            }
            let _: Token![=] = input.parse().map_err(|e| Error::new(e.span(), ONE_KEY))?;
            let lit: LitStr = input.parse().map_err(|e| Error::new(e.span(), ONE_KEY))?;
            if !input.is_empty() {
                return Err(input.error(ONE_KEY));
            }
            Ok(lit)
        })?;
        found = Some(lit);
    }
    Ok(found)
}

/// Is `name` one word a user can type as a value?
fn is_word(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(char::is_whitespace) && !name.starts_with('-')
}

/// The default name: the ident, unescaped, snake_cased the way
/// `#[derive(Bpaf)]` does it, with `_` → `-`.
fn kebab(ident: &Ident) -> String {
    snake_case(&ident.unraw())
        .unraw()
        .to_string()
        .replace('_', "-")
}

/// The first non-blank line of the doc comment, trimmed.
fn description(attrs: &[Attribute]) -> Option<String> {
    attrs
        .iter()
        .filter_map(doc_text)
        .flat_map(|t| {
            t.split('\n')
                .map(|l| l.trim().to_string())
                .collect::<Vec<_>>()
        })
        .find(|l| !l.is_empty())
}

/// Expand `#[derive(Choice)]`, or explain why this type cannot be a choice.
pub(crate) fn expand(input: DeriveInput) -> Result<TokenStream> {
    let Data::Enum(data) = &input.data else {
        return Err(Error::new_spanned(&input.ident, NOT_AN_ENUM));
    };
    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(&input.generics, GENERIC));
    }
    if let Some(w) = &input.generics.where_clause {
        return Err(Error::new_spanned(w, GENERIC));
    }
    if let Some(attr) = input.attrs.iter().find(|a| a.path().is_ident("choice")) {
        return Err(Error::new_spanned(attr, ON_THE_ENUM));
    }
    if data.variants.is_empty() {
        return Err(Error::new_spanned(&input.ident, NO_VARIANTS));
    }

    let mut variants: Vec<Variant> = Vec::with_capacity(data.variants.len());
    for v in &data.variants {
        if !matches!(v.fields, Fields::Unit) {
            return Err(Error::new_spanned(&v.fields, VARIANT_WITH_FIELDS));
        }
        let (name, at) = match read_name(&v.attrs)? {
            Some(lit) => (lit.value(), lit.span()),
            None => (kebab(&v.ident), v.ident.span()),
        };
        if !is_word(&name) {
            return Err(Error::new(at, BAD_NAME));
        }
        if variants.iter().any(|seen| seen.name == name) {
            return Err(Error::new(at, duplicate(&name)));
        }
        variants.push(Variant {
            ident: v.ident.clone(),
            name,
            at,
            description: description(&v.attrs),
        });
    }

    let ty = &input.ident;
    let paths: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let id = &v.ident;
            quote!(#ty::#id)
        })
        .collect();
    let names: Vec<LitStr> = variants
        .iter()
        .map(|v| LitStr::new(&v.name, v.at))
        .collect();
    let descriptions: Vec<TokenStream> = variants
        .iter()
        .map(|v| match &v.description {
            Some(d) => quote!(::core::option::Option::Some(#d)),
            None => quote!(::core::option::Option::None),
        })
        .collect();

    Ok(quote! {
        #[automatically_derived]
        impl ::hicli::Choice for #ty {
            const VARIANTS: &'static [Self] = &[#(#paths),*];
            fn name(&self) -> &'static str {
                match self { #(#paths => #names),* }
            }
            fn description(&self) -> ::core::option::Option<&'static str> {
                match self { #(#paths => #descriptions),* }
            }
        }
        #[automatically_derived]
        impl ::core::str::FromStr for #ty {
            type Err = ::hicli::UnknownChoice;
            fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
                match s {
                    #(#names => ::core::result::Result::Ok(#paths),)*
                    _ => ::core::result::Result::Err(::hicli::UnknownChoice::new::<Self>(s)),
                }
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Display for #ty {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(<Self as ::hicli::Choice>::name(self))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expand `src` to a token string.
    fn expansion(src: &str) -> String {
        let input: DeriveInput = syn::parse_str(src).expect("the type parses");
        expand(input).expect("is a choice").to_string()
    }

    /// The error `#[derive(Choice)]` raises for `src`.
    fn rejection(src: &str) -> String {
        let input: DeriveInput = syn::parse_str(src).expect("the type parses");
        expand(input).expect_err("is not a choice").to_string()
    }

    /// The names the derive gives `src`'s variants, read off `VARIANTS`'
    /// `name` match arms.
    fn names(src: &str) -> Vec<String> {
        let input: DeriveInput = syn::parse_str(src).expect("the type parses");
        let Data::Enum(data) = &input.data else {
            panic!("an enum")
        };
        data.variants
            .iter()
            .map(|v| match read_name(&v.attrs).expect("reads") {
                Some(lit) => lit.value(),
                None => kebab(&v.ident),
            })
            .collect()
    }

    #[test]
    fn the_whole_expansion() {
        let got = expansion(
            "pub enum SortKey {\n\
             /// Beats per minute\n\
             Bpm,\n\
             Title,\n\
             #[choice(name = \"by-artist\")]\n\
             Artist,\n\
             }",
        );
        let want = quote! {
            #[automatically_derived]
            impl ::hicli::Choice for SortKey {
                const VARIANTS: &'static [Self] = &[SortKey::Bpm, SortKey::Title, SortKey::Artist];
                fn name(&self) -> &'static str {
                    match self { SortKey::Bpm => "bpm", SortKey::Title => "title", SortKey::Artist => "by-artist" }
                }
                fn description(&self) -> ::core::option::Option<&'static str> {
                    match self {
                        SortKey::Bpm => ::core::option::Option::Some("Beats per minute"),
                        SortKey::Title => ::core::option::Option::None,
                        SortKey::Artist => ::core::option::Option::None
                    }
                }
            }
            #[automatically_derived]
            impl ::core::str::FromStr for SortKey {
                type Err = ::hicli::UnknownChoice;
                fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
                    match s {
                        "bpm" => ::core::result::Result::Ok(SortKey::Bpm),
                        "title" => ::core::result::Result::Ok(SortKey::Title),
                        "by-artist" => ::core::result::Result::Ok(SortKey::Artist),
                        _ => ::core::result::Result::Err(::hicli::UnknownChoice::new::<Self>(s)),
                    }
                }
            }
            #[automatically_derived]
            impl ::core::fmt::Display for SortKey {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    f.write_str(<Self as ::hicli::Choice>::name(self))
                }
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_variant_is_named_in_kebab_case() {
        assert_eq!(
            names("enum K { PlayCount, HTTPServer, Mp3, Bpm, r#Type }"),
            ["play-count", "http-server", "mp3", "bpm", "type"]
        );
    }

    #[test]
    fn a_name_overrides_the_ident() {
        assert_eq!(
            names("enum K { #[choice(name = \"by-artist\")] Artist, Title }"),
            ["by-artist", "title"]
        );
        // The override is what `FromStr` matches, too.
        let got = expansion("enum K { #[choice(name = \"x\")] Long }");
        assert!(
            got.contains("\"x\" => :: core :: result :: Result :: Ok (K :: Long)"),
            "{got}"
        );
    }

    #[test]
    fn the_description_is_the_first_non_blank_doc_line() {
        let input: DeriveInput = syn::parse_str(
            "enum K {\n\
             ///\n\
             ///   Beats per minute  \n\
             /// More prose.\n\
             Bpm,\n\
             ///\n\
             Title,\n\
             Artist,\n\
             }",
        )
        .expect("parses");
        let Data::Enum(data) = &input.data else {
            panic!("an enum")
        };
        let got: Vec<Option<String>> = data
            .variants
            .iter()
            .map(|v| description(&v.attrs))
            .collect();
        assert_eq!(got, [Some("Beats per minute".to_string()), None, None]);
    }

    #[test]
    fn an_explicit_discriminant_is_allowed() {
        let got = expansion("enum K { A = 1, B = 5 }");
        assert!(got.contains("\"a\""), "{got}");
        assert!(got.contains("\"b\""), "{got}");
    }

    #[test]
    fn c1_a_struct_or_union_is_rejected() {
        assert_eq!(rejection("struct K;"), NOT_AN_ENUM);
        assert_eq!(rejection("struct K(String);"), NOT_AN_ENUM);
        assert_eq!(rejection("union K { a: u8 }"), NOT_AN_ENUM);
    }

    #[test]
    fn c2_a_variant_with_fields_is_rejected() {
        assert_eq!(rejection("enum K { A, B(u8) }"), VARIANT_WITH_FIELDS);
        assert_eq!(rejection("enum K { A { x: u8 } }"), VARIANT_WITH_FIELDS);
    }

    #[test]
    fn c3_an_empty_enum_is_rejected() {
        assert_eq!(rejection("enum K {}"), NO_VARIANTS);
    }

    #[test]
    fn c4_a_generic_enum_is_rejected() {
        assert_eq!(rejection("enum K<T> { A, B(T) }"), GENERIC);
        assert_eq!(rejection("enum K where u8: Copy { A }"), GENERIC);
    }

    #[test]
    fn c5_two_variants_spelled_alike_are_rejected() {
        assert_eq!(
            rejection("enum K { Artist, #[choice(name = \"artist\")] ByArtist }"),
            "two choices are both spelled `artist`"
        );
        assert_eq!(
            rejection("enum K { #[choice(name = \"x\")] A, #[choice(name = \"x\")] B }"),
            "two choices are both spelled `x`"
        );
    }

    #[test]
    fn c6_a_name_that_is_not_one_word_is_rejected() {
        for bad in ["", "by artist", "-x", "--x", "a\tb"] {
            assert_eq!(
                rejection(&format!("enum K {{ #[choice(name = {bad:?})] A }}")),
                BAD_NAME,
                "{bad:?}"
            );
        }
        // A derived name is held to the same rule: `_Hidden` would be `--hidden`.
        assert_eq!(rejection("enum K { _Hidden }"), BAD_NAME);
    }

    #[test]
    fn c7_anything_but_one_name_key_is_rejected() {
        for bad in [
            "#[choice(alias = \"x\")] A",
            "#[choice(name = \"x\", name = \"y\")] A",
            "#[choice(name = \"x\", hide)] A",
            "#[choice(name)] A",
            "#[choice(name = x)] A",
            "#[choice(\"x\")] A",
            "#[choice()] A",
            "#[choice] A",
            "#[choice = \"x\"] A",
            "#[choice(name = \"x\")] #[choice(name = \"y\")] A",
        ] {
            assert_eq!(rejection(&format!("enum K {{ {bad} }}")), ONE_KEY, "{bad}");
        }
    }

    #[test]
    fn c8_choice_on_the_enum_is_rejected() {
        assert_eq!(
            rejection("#[choice(name = \"k\")] enum K { A }"),
            ON_THE_ENUM
        );
        assert_eq!(rejection("#[choice] enum K { A }"), ON_THE_ENUM);
    }
}
