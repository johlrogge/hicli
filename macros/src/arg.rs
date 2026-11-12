//! `#[derive(Arg)]`: the impl that says how an argument type parses.
//!
//! Two modes, chosen by the keys of `#[arg(…)]`:
//!
//! - **bpaf mode** — no `positional`, `long` or `short`. The whole derive is
//!   one sentence — *`Id`'s parser is `id()`* — plus an optional completer
//!   wrapped around it. The constructor is the one `#[derive(Bpaf)]` writes,
//!   named by snake-casing the type's ident. The call is **spanned at the
//!   type's ident**: a type that derives `Arg` without `Bpaf` gets rustc's own
//!   *"cannot find function `id`"* pointing at `Id`.
//! - **spelled mode** — any of `positional`, `long` or `short`. The derive
//!   spells the argument itself, so the type needs no `#[derive(Bpaf)]` and its
//!   crate no direct `bpaf` dependency. It writes three items: the snake_case
//!   constructor `#[derive(Bpaf)]` would have written (so
//!   `#[bpaf(external(…))]` still finds it), the `Arg` impl calling it, and an
//!   `ArgHelp` impl that builds the parser around a given help — the type's
//!   doc comment's first paragraph by default, a command's `# Arguments`
//!   entry when it has one.
//!
//! The completion helpers are emitted as **functions**
//! (`::hicli::complete_with(…)`), never as methods on the parser, so the
//! bpaf-mode expansion needs no trait in scope and reads as what the author
//! could have written by hand.

use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Error, Fields, GenericArgument, Ident, LitChar, LitStr, Meta,
    Path, PathArguments, Result, Token, Type,
};

use crate::shared::{
    doc_lines, first_paragraph, is_cfg, last_segment, not_already_optional, option_inner,
    qualified, qualified_call, snake_case, DOUBLE_OPTION,
};

/// The message for an `#[arg(…)]` key it does not know (A1), listing every key.
const UNKNOWN_ARG: &str = "#[arg(...)] takes `positional`, `long`, `short`, `metavar`, `env` \
                           and `complete`: `positional` or `positional = \"KEY\"`, `long` or \
                           `long = \"name\"`, `short = 'c'`, `metavar = \"M\"`, \
                           `env = \"VAR\"`, and `complete = <fn>`, `complete = files` or \
                           `complete = dirs`";

/// `env` with `positional` (ADR-019 §5).
const ENV_ON_POSITIONAL: &str =
    "a positional cannot fall back to an environment variable: `env` goes with `long` or `short`";

/// `env` on a switch (ADR-019 §5).
const ENV_ON_SWITCH: &str =
    "a switch takes no value, so it has no environment fallback: remove `env` from this `bool`";

/// `env` without `long`/`short`, in bpaf or composed mode (ADR-019 §5).
const ENV_ALONE: &str = "`env` names the environment variable an option falls back to: it goes \
                         with `long` or `short`";

/// An `env` that cannot name a variable (ADR-019 §5).
const BAD_ENV: &str = "`env` is the variable's name: one non-empty word with no `=`";

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

/// S1: a spelled type with no doc comment, or a blank one.
const NEEDS_DOC: &str = "an argument type spelled by #[arg(...)] needs a doc comment: its first \
                         paragraph is the help for its row";

/// S2: `positional` together with `long` or `short`.
const POSITIONAL_AND_NAMED: &str = "`positional` cannot be combined with `long` or `short`: an \
                                    argument is either a positional or an option";

/// S3: a positional `bool`.
const POSITIONAL_BOOL: &str = "a `bool` cannot be a positional: a `bool` is a switch, so spell it \
                               with `long` and/or `short`";

/// S4a: `metavar` on a switch.
const METAVAR_ON_BOOL: &str =
    "a switch takes no value, so it has no metavar: remove `metavar` from this `bool`";

/// S4b: `metavar` with `positional`.
const METAVAR_ON_POSITIONAL: &str =
    "a positional's metavar is its own value: write `positional = \"…\"` instead of `metavar`";

/// S4c: `metavar` without a spelling key, in bpaf mode.
const METAVAR_ALONE: &str =
    "`metavar` names an option's value: it goes with `long` or `short` (a positional writes \
     `positional = \"…\"`)";

/// S5: an option with a value but no metavar.
const NEEDS_METAVAR: &str = "an option with a value needs `metavar = \"…\"`: the word that stands \
                             for its value in help and usage, as in `--sort=FIELD`";

/// S6: `Option<bool>` / `Vec<bool>`.
const OPTIONAL_SWITCH: &str = "a switch is a plain `bool`, `false` when absent: spell \
                               `Option<bool>` and `Vec<bool>` as `bool`";

/// S7: a spelled type of a shape that cannot be one word.
const SPELLED_SHAPE: &str = "#[arg(positional|long|short …)] spells a struct — a newtype, a \
                             struct with one named field, or a struct with several fields that \
                             implements FromStr — or a fieldless enum that derives Choice";

/// S8: `#[bpaf(…)]` attributes on a spelled type.
const BPAF_ATTRS: &str = "#[arg(positional…)] spells this type: remove #[derive(Bpaf)] and its \
                          #[bpaf(…)] attributes";

/// S9a: a `short` that cannot be a flag.
const BAD_SHORT: &str = "`short` is one character that is not `-` or whitespace: `short = 'c'`";

/// S9b: a `long` that cannot be a flag.
const BAD_LONG: &str = "`long` is the option's name without its dashes: one non-empty word that \
                        does not start with `-`";

/// S11: `complete` on a switch.
const COMPLETE_ON_BOOL: &str = "a switch takes no value, so there is nothing to complete: remove \
                                `complete` from this `bool`";

/// `#[arg(…)]` on a field, in any mode (ADR-019 §2).
const FIELD_ARG: &str = "#[arg(...)] goes on a type, never on a field: a field is an argument \
                         type. Spell the value on a newtype (`/// help` above `#[derive(Arg)] \
                         #[arg(long = \"name\", metavar = \"M\")] struct Name(Value);`), make \
                         that newtype the field's type, and write `Option<Name>` where it may \
                         be left out";

/// `#[cfg]` / `#[cfg_attr]` on a composed struct's field.
const COMPOSED_CFG: &str = "#[cfg] and #[cfg_attr] cannot go on a field of a struct \
                            #[derive(Arg)] composes: the composition names every field. Put the \
                            #[cfg] on the whole struct, or give each configuration its own struct";

/// A composed struct with no fields (ADR-019 §1).
const COMPOSED_EMPTY: &str = "#[derive(Arg)] composes a struct's fields, and this one has none: \
                              an argument type with no value has nothing to parse";

/// `complete` on a composed struct (ADR-019 §1).
const COMPOSED_COMPLETE: &str = "#[arg(complete = …)] on a composed struct has no one value to \
                                 complete: each field completes through its own type";

/// S10: a key given twice.
fn twice(key: &Ident) -> String {
    format!("`{}` is given twice", key.unraw())
}

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

/// One key, with where it was written.
struct Key<T> {
    /// The key's ident — the span its rejections point at.
    at: Ident,
    /// What it was given.
    value: T,
}

/// What `#[arg(…)]` was given.
#[derive(Default)]
struct ArgArgs {
    /// `positional` / `positional = "KEY"`.
    positional: Option<Key<Option<LitStr>>>,
    /// `long` / `long = "name"`.
    long: Option<Key<Option<LitStr>>>,
    /// `short = 'c'`.
    short: Option<Key<LitChar>>,
    /// `metavar = "M"`.
    metavar: Option<Key<LitStr>>,
    /// `env = "VAR"`.
    env: Option<Key<LitStr>>,
    /// `complete = …`.
    complete: Option<Key<Completion>>,
}

impl ArgArgs {
    /// Is this spelled mode?
    fn spelled(&self) -> bool {
        self.positional.is_some() || self.long.is_some() || self.short.is_some()
    }

    /// The completer asked for.
    fn completion(&self) -> &Completion {
        self.complete
            .as_ref()
            .map_or(&Completion::None, |k| &k.value)
    }
}

/// Store `value` under `slot`, or reject the key as given twice.
fn set<T>(slot: &mut Option<Key<T>>, at: Ident, value: T) -> Result<()> {
    if slot.is_some() {
        return Err(Error::new_spanned(&at, twice(&at)));
    }
    *slot = Some(Key { at, value });
    Ok(())
}

/// An optional `= "…"` after a bare-able key.
fn optional_str(input: ParseStream) -> Result<Option<LitStr>> {
    if input.peek(Token![=]) {
        let _: Token![=] = input.parse()?;
        let lit: LitStr = input
            .parse()
            .map_err(|e| Error::new(e.span(), UNKNOWN_ARG))?;
        Ok(Some(lit))
    } else {
        Ok(None)
    }
}

/// A required `= <value>` after a key.
fn required<T: Parse>(input: ParseStream) -> Result<T> {
    let _: Token![=] = input
        .parse()
        .map_err(|e| Error::new(e.span(), UNKNOWN_ARG))?;
    input.parse().map_err(|e| Error::new(e.span(), UNKNOWN_ARG))
}

impl Parse for ArgArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut args = ArgArgs::default();
        while !input.is_empty() {
            let key = input
                .call(Ident::parse_any)
                .map_err(|e| Error::new(e.span(), UNKNOWN_ARG))?;
            if key == "positional" {
                let v = optional_str(input)?;
                set(&mut args.positional, key, v)?;
            } else if key == "long" {
                let v = optional_str(input)?;
                if let Some(l) = &v {
                    let s = l.value();
                    if s.is_empty() || s.chars().any(char::is_whitespace) || s.starts_with('-') {
                        return Err(Error::new_spanned(l, BAD_LONG));
                    }
                }
                set(&mut args.long, key, v)?;
            } else if key == "short" {
                let c: LitChar = required(input)?;
                if c.value() == '-' || c.value().is_whitespace() {
                    return Err(Error::new_spanned(&c, BAD_SHORT));
                }
                set(&mut args.short, key, c)?;
            } else if key == "metavar" {
                let m: LitStr = required(input)?;
                set(&mut args.metavar, key, m)?;
            } else if key == "env" {
                let e: LitStr = required(input)?;
                let s = e.value();
                if s.is_empty() || s.chars().any(char::is_whitespace) || s.contains('=') {
                    return Err(Error::new_spanned(&e, BAD_ENV));
                }
                set(&mut args.env, key, e)?;
            } else if key == "complete" {
                let path: Path = required(input)?;
                set(&mut args.complete, key, shorthand(path))?;
            } else {
                return Err(Error::new_spanned(&key, UNKNOWN_ARG));
            }
            if input.is_empty() {
                break;
            }
            let _: Token![,] = input
                .parse()
                .map_err(|e| Error::new(e.span(), UNKNOWN_ARG))?;
        }
        Ok(args)
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
fn read_attrs(attrs: &[Attribute]) -> Result<(ArgArgs, Option<Attribute>)> {
    let mut found = (ArgArgs::default(), None);
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
                found = (args, Some(attr.clone()));
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

    if let Some(a) = field_arg_attr(&input) {
        return Err(Error::new_spanned(a, FIELD_ARG));
    }

    let (args, at) = read_attrs(&input.attrs)?;
    if args.spelled() {
        let at = at.expect("spelling keys were given by an attribute");
        return spelled(&input, args, &at);
    }
    if let Data::Struct(s) = &input.data {
        if let Fields::Named(fields) = &s.fields {
            if any_bpaf_attr(&input).is_none() {
                return composed(&input, fields, &args, at.as_ref());
            }
        }
    }
    if let Some(m) = &args.metavar {
        return Err(Error::new_spanned(&m.at, METAVAR_ALONE));
    }
    if let Some(e) = &args.env {
        return Err(Error::new_spanned(&e.at, ENV_ALONE));
    }
    let completion = args.completion();
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

/// The first `#[arg]` on a field — of the struct, or of any variant — or on a
/// variant itself.
fn field_arg_attr(input: &DeriveInput) -> Option<&Attribute> {
    fn arg(attrs: &[Attribute]) -> Option<&Attribute> {
        attrs.iter().find(|a| a.path().is_ident("arg"))
    }
    fn on(f: &syn::Field) -> Option<&Attribute> {
        arg(&f.attrs)
    }
    match &input.data {
        Data::Struct(s) => s.fields.iter().find_map(on),
        Data::Enum(e) => e
            .variants
            .iter()
            .find_map(|v| arg(&v.attrs).or_else(|| v.fields.iter().find_map(on))),
        Data::Union(u) => u.fields.named.iter().find_map(on),
    }
}

// --- composed mode -----------------------------------------------------------

/// Expand a struct of argument types (ADR-019): its snake_case constructor,
/// composing each field's parser with `construct!` in declaration order, and
/// the `Arg` impl calling it.
fn composed(
    input: &DeriveInput,
    fields: &syn::FieldsNamed,
    args: &ArgArgs,
    at: Option<&Attribute>,
) -> Result<TokenStream> {
    if fields.named.is_empty() {
        return Err(Error::new_spanned(&input.ident, COMPOSED_EMPTY));
    }
    if let Some(m) = &args.metavar {
        return Err(Error::new_spanned(&m.at, METAVAR_ALONE));
    }
    if args.complete.is_some() {
        let at = at.expect("a completion was asked for by an attribute");
        return Err(Error::new_spanned(at, COMPOSED_COMPLETE));
    }
    if let Some(e) = &args.env {
        return Err(Error::new_spanned(&e.at, ENV_ALONE));
    }
    for f in &fields.named {
        if let Some(c) = f.attrs.iter().find(|a| is_cfg(a)) {
            return Err(Error::new_spanned(c, COMPOSED_CFG));
        }
        if option_inner(&f.ty).and_then(option_inner).is_some() {
            return Err(Error::new_spanned(&f.ty, DOUBLE_OPTION));
        }
    }

    let idents: Vec<&Ident> = fields
        .named
        .iter()
        .map(|f| f.ident.as_ref().expect("a named field has an ident"))
        .collect();
    let lets = fields.named.iter().zip(&idents).map(|(f, ident)| {
        let parser = field_parser(f);
        quote!(let #ident = #parser;)
    });
    // One assertion per `Option<T>` field, beside the constructor. A generic
    // composite would need a skip here — a const item cannot name the outer
    // type's parameters — but `#[derive(Arg)]` refuses generics (A4).
    let checks = fields
        .named
        .iter()
        .filter_map(|f| option_inner(&f.ty))
        .map(not_already_optional);
    // A composite succeeds without any argument exactly when every field does:
    // an `Option<T>` field always does, a plain field when its type does.
    let optional = fields
        .named
        .iter()
        .filter(|f| option_inner(&f.ty).is_none())
        .map(|f| qualified(&f.ty, quote!(::hicli::Arg), quote!(OPTIONAL)));
    let heading = first_paragraph(&doc_lines(&input.attrs)).map(|(h, _)| quote!(.group_help(#h)));

    let ty = &input.ident;
    let ctor = snake_case(ty);
    let vis = &input.vis;
    let ctor_doc = format!("The parser `#[derive(Arg)]` wrote for [`{ty}`].");
    Ok(quote! {
        #[doc = #ctor_doc]
        #[doc(hidden)]
        #vis fn #ctor() -> impl ::hicli::bpaf::Parser<#ty> {
            #[allow(unused_imports)]
            use ::hicli::bpaf::Parser as _;
            #(#lets)*
            ::hicli::bpaf::construct!(#ty { #(#idents),* }) #heading
        }
        #(#checks)*
        #[automatically_derived]
        impl ::hicli::Arg for #ty {
            const OPTIONAL: bool = true #(&& #optional)*;
            fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                #ctor()
            }
        }
    })
}

/// One field's parser: its type's, or — with a doc comment — its type's
/// built around the doc's first paragraph, through `ArgHelp`; for an
/// `Option<T>` field, `T`'s made optional. Spanned at the type, so a type
/// that is not an argument type is reported on the field.
fn field_parser(field: &syn::Field) -> TokenStream {
    let help = first_paragraph(&doc_lines(&field.attrs)).map(|(h, _)| h);
    match option_inner(&field.ty) {
        Some(inner) => {
            let p = type_parser(inner, help.as_deref());
            quote_spanned!(inner.span()=> #p.optional())
        }
        None => type_parser(&field.ty, help.as_deref()),
    }
}

/// `<T as Arg>::parser()`, or `<T as ArgHelp>::parser_with_help(help)`,
/// spanned over the whole of `T` (see `qualified`).
fn type_parser(ty: &Type, help: Option<&str>) -> TokenStream {
    match help {
        None => qualified_call(ty, quote!(::hicli::Arg), quote!(parser), quote!()),
        Some(h) => qualified_call(
            ty,
            quote!(::hicli::ArgHelp),
            quote!(parser_with_help),
            quote!(#h),
        ),
    }
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

// --- spelled mode ------------------------------------------------------------

/// Where the value goes once parsed: the shape of the spelled type.
enum Shape<'a> {
    /// `struct T(Inner);` — `.map(T)`.
    Tuple(&'a Type),
    /// `struct T { field: Inner }` — `.map(|v| T { field: v })`.
    Named(&'a Ident, &'a Type),
    /// A fieldless enum: the type *is* the value, and a `Choice`.
    Choice,
    /// A struct with several fields: one word, parsed whole by its own
    /// `FromStr` (ADR-018).
    Whole,
}

/// How many values the field holds, read off its type's last path segment.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arity {
    /// `T` — exactly one.
    One,
    /// `Option<T>` — at most one.
    Optional,
    /// `Vec<T>` — any number.
    Many,
}

/// `Option<T>` / `Vec<T>` → the arity and `T`, recognised syntactically as
/// `bpaf_derive` does; anything else is one value of itself.
fn arity(ty: &Type) -> (Arity, &Type) {
    if let Some(inner) = option_inner(ty) {
        return (Arity::Optional, inner);
    }
    let Some(seg) = last_segment(ty) else {
        return (Arity::One, ty);
    };
    if seg.ident != "Vec" {
        return (Arity::One, ty);
    }
    let kind = Arity::Many;
    let PathArguments::AngleBracketed(a) = &seg.arguments else {
        return (Arity::One, ty);
    };
    match (a.args.len(), a.args.first()) {
        (1, Some(GenericArgument::Type(inner))) => (kind, inner),
        _ => (Arity::One, ty),
    }
}

/// Is `ty` spelled as the plain path `…::name`, with no generic arguments?
fn is_named(ty: &Type, name: &str) -> bool {
    last_segment(ty).is_some_and(|s| s.ident == name && matches!(s.arguments, PathArguments::None))
}

/// Does `attrs` hold a `#[bpaf(…)]`?
fn bpaf_attr(attrs: &[Attribute]) -> Option<&Attribute> {
    attrs.iter().find(|a| a.path().is_ident("bpaf"))
}

/// The spelled type's shape, or why it cannot be spelled.
fn shape<'a>(input: &'a DeriveInput, at: &Attribute) -> Result<Shape<'a>> {
    match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Unnamed(f) if f.unnamed.len() == 1 => Ok(Shape::Tuple(&f.unnamed[0].ty)),
            Fields::Named(f) if f.named.len() == 1 => {
                let field = &f.named[0];
                let ident = field.ident.as_ref().expect("a named field has an ident");
                Ok(Shape::Named(ident, &field.ty))
            }
            Fields::Unnamed(f) if f.unnamed.len() > 1 => Ok(Shape::Whole),
            Fields::Named(f) if f.named.len() > 1 => Ok(Shape::Whole),
            _ => Err(Error::new_spanned(at, SPELLED_SHAPE)),
        },
        Data::Enum(e) => {
            if e.variants.iter().all(|v| matches!(v.fields, Fields::Unit)) {
                Ok(Shape::Choice)
            } else {
                Err(Error::new_spanned(at, SPELLED_SHAPE))
            }
        }
        Data::Union(_) => Err(Error::new_spanned(&input.ident, A_UNION)),
    }
}

/// Every `#[bpaf(…)]` anywhere on the type: on it, its fields or its variants.
fn any_bpaf_attr(input: &DeriveInput) -> Option<&Attribute> {
    if let Some(a) = bpaf_attr(&input.attrs) {
        return Some(a);
    }
    match &input.data {
        Data::Struct(s) => s.fields.iter().find_map(|f| bpaf_attr(&f.attrs)),
        Data::Enum(e) => e.variants.iter().find_map(|v| {
            bpaf_attr(&v.attrs).or_else(|| v.fields.iter().find_map(|f| bpaf_attr(&f.attrs)))
        }),
        Data::Union(_) => None,
    }
}

/// `SortBy` → `SORT-BY`: a bare `positional`'s metavar.
fn screaming_kebab(ident: &Ident) -> String {
    kebab(ident).to_uppercase()
}

/// `SortBy` → `sort-by`: a bare `long`'s name.
fn kebab(ident: &Ident) -> String {
    snake_case(&ident.unraw())
        .unraw()
        .to_string()
        .replace('_', "-")
}

/// Expand a spelled type into its three items.
fn spelled(input: &DeriveInput, args: ArgArgs, at: &Attribute) -> Result<TokenStream> {
    if let Some(a) = any_bpaf_attr(input) {
        return Err(Error::new_spanned(a, BPAF_ATTRS));
    }
    let shape = shape(input, at)?;
    let help = match first_paragraph(&doc_lines(&input.attrs)) {
        Some((help, _)) => help,
        None => return Err(Error::new_spanned(&input.ident, NEEDS_DOC)),
    };
    if let Some(p) = &args.positional {
        if args.long.is_some() || args.short.is_some() {
            return Err(Error::new_spanned(&p.at, POSITIONAL_AND_NAMED));
        }
        if let Some(m) = &args.metavar {
            return Err(Error::new_spanned(&m.at, METAVAR_ON_POSITIONAL));
        }
        if let Some(e) = &args.env {
            return Err(Error::new_spanned(&e.at, ENV_ON_POSITIONAL));
        }
    }

    let ty = &input.ident;
    let (arity, inner) = match &shape {
        Shape::Tuple(t) | Shape::Named(_, t) => {
            let (a, i) = arity(t);
            (a, Some(i))
        }
        Shape::Choice | Shape::Whole => (Arity::One, None),
    };
    let switch = inner.is_some_and(|i| is_named(i, "bool"));
    if switch {
        if arity != Arity::One {
            let t = match &shape {
                Shape::Tuple(t) | Shape::Named(_, t) => *t,
                Shape::Choice | Shape::Whole => unreachable!("only a field is a switch"),
            };
            return Err(Error::new_spanned(t, OPTIONAL_SWITCH));
        }
        if let Some(p) = &args.positional {
            return Err(Error::new_spanned(&p.at, POSITIONAL_BOOL));
        }
        if let Some(m) = &args.metavar {
            return Err(Error::new_spanned(&m.at, METAVAR_ON_BOOL));
        }
        if let Some(c) = &args.complete {
            return Err(Error::new_spanned(&c.at, COMPLETE_ON_BOOL));
        }
        if let Some(e) = &args.env {
            return Err(Error::new_spanned(&e.at, ENV_ON_SWITCH));
        }
    } else if args.positional.is_none() && args.metavar.is_none() {
        let k = args.long.as_ref().map_or_else(
            || &args.short.as_ref().expect("long or short").at,
            |l| &l.at,
        );
        return Err(Error::new_spanned(k, NEEDS_METAVAR));
    }

    let chain = chain(input, &args, &shape, arity, inner, switch);
    let ctor = snake_case(ty);
    let vis = &input.vis;
    let ctor_doc = format!("The parser `#[derive(Arg)]` wrote for [`{ty}`].");
    let help = LitStr::new(&help, Span::call_site());
    // A switch, an `Option` or a `Vec` succeeds without its argument.
    let optional = (switch || arity != Arity::One).then(|| {
        quote!(
            const OPTIONAL: bool = true;
        )
    });

    Ok(quote! {
        #[doc = #ctor_doc]
        #[doc(hidden)]
        #vis fn #ctor() -> impl ::hicli::bpaf::Parser<#ty> {
            <#ty as ::hicli::ArgHelp>::parser_with_help(#help)
        }
        #[automatically_derived]
        impl ::hicli::Arg for #ty {
            #optional
            fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                #ctor()
            }
        }
        #[automatically_derived]
        impl ::hicli::ArgHelp for #ty {
            fn parser_with_help(help: &'static str) -> impl ::hicli::bpaf::Parser<Self> {
                #chain
            }
        }
    })
}

/// The body of `parser_with_help`: the bpaf chain for this spelling.
fn chain(
    input: &DeriveInput,
    args: &ArgArgs,
    shape: &Shape<'_>,
    arity: Arity,
    inner: Option<&Type>,
    switch: bool,
) -> TokenStream {
    let ty = &input.ident;
    let string = quote!(::std::string::String);
    let completion = args.completion();
    // The name a parse error is prefixed with (ADR-018).
    let name = LitStr::new(&spelling(args, ty), Span::call_site());
    // The variable an error from the environment names (ADR-019 §5).
    let env = args.env.as_ref().map(|e| &e.value);

    let named = || {
        let long = args.long.as_ref().map(|l| match &l.value {
            Some(s) => quote!(#s),
            None => {
                let s = LitStr::new(&kebab(ty), l.at.span());
                quote!(#s)
            }
        });
        let short = args.short.as_ref().map(|s| &s.value);
        let head = match (long, short) {
            (Some(l), Some(s)) => quote!(::hicli::bpaf::long(#l).short(#s)),
            (Some(l), None) => quote!(::hicli::bpaf::long(#l)),
            (None, Some(s)) => quote!(::hicli::bpaf::short(#s)),
            (None, None) => unreachable!("a named argument has a long or a short"),
        };
        // Straight after the name, before `.help`, as bpaf's derive writes it.
        match &args.env {
            Some(e) => {
                let e = &e.value;
                quote!(#head.env(#e))
            }
            None => head,
        }
    };

    // The head: `positional::<P>(META).help(H)` or `long(…).help(H).argument::<P>(META)`.
    let head = |parsed_as: &TokenStream, help: TokenStream| match &args.positional {
        Some(p) => {
            let meta = match &p.value {
                Some(m) => m.clone(),
                None => LitStr::new(&screaming_kebab(ty), p.at.span()),
            };
            quote!(::hicli::bpaf::positional::<#parsed_as>(#meta).help(#help))
        }
        None => {
            let n = named();
            let meta = &args
                .metavar
                .as_ref()
                .expect("checked: an option has a metavar")
                .value;
            quote!(#n.help(#help).argument::<#parsed_as>(#meta))
        }
    };

    let body = match shape {
        _ if switch => {
            let n = named();
            let map = wrap(ty, shape);
            quote!(#n.help(help).switch().map(#map))
        }
        // One `Choice` bound for the whole chain, so an enum that is not a
        // choice is one E0277, not that and a `FromStr` one.
        Shape::Choice => {
            let h = head(&string, quote!(help));
            match env {
                None => quote!(::hicli::__private::choice::<Self, _>(#name, help, |help| #h)),
                Some(var) => quote!(
                    ::hicli::__private::choice_env::<Self, _>(#name, #var, help, |help| #h)
                ),
            }
        }
        Shape::Whole => {
            let h = head(&string, quote!(help));
            let h = match completion {
                Completion::None => h,
                Completion::Files => quote!(::hicli::complete_files(#h)),
                Completion::Dirs => quote!(::hicli::complete_dirs(#h)),
                Completion::With(f) => quote!(#h.complete(|w: &#string| #f(w.as_str()))),
            };
            // Spanned at the type: a missing `FromStr` is reported on it.
            let closure = parse_closure(
                ty.span(),
                &string,
                quote_spanned!(ty.span()=> w.parse::<Self>()),
                &name,
                env,
            );
            let parse = quote_spanned! {ty.span()=> .parse(#closure) };
            quote!(#h #parse)
        }
        Shape::Tuple(field) | Shape::Named(_, field) => {
            let value = inner.expect("a field has a value type");
            return field_chain(ty, shape, field, value, arity, completion, &name, env, head);
        }
    };
    quote! {
        #[allow(unused_imports)]
        use ::hicli::bpaf::Parser as _;
        #body
    }
}

/// The parse closure `|w: P| <parse>.map_err(…)`, its error named by `name`.
///
/// With an `env` the closure first asks whether `w` came from the
/// environment, before `parse` may move it, and names the error
/// `` `--addr` from `$VAR`: … `` then — never echoing the value. Without one
/// it is exactly the 0.6 closure. Spanned at `span`, as the caller's was.
fn parse_closure(
    span: Span,
    param: &TokenStream,
    parse: TokenStream,
    name: &LitStr,
    env: Option<&LitStr>,
) -> TokenStream {
    match env {
        None => quote_spanned! {span=>
            |w: #param| #parse.map_err(|e| ::hicli::__private::named(#name, e))
        },
        Some(var) => quote_spanned! {span=>
            |w: #param| {
                let __env = ::hicli::__private::from_env(
                    #var,
                    ::core::convert::AsRef::<::std::ffi::OsStr>::as_ref(&w),
                );
                #parse.map_err(|e| ::hicli::__private::named_env(#name, #var, __env, e))
            }
        },
    }
}

/// The function that wraps a field's value into the type: `T` or
/// `|v| T { field: v }`.
fn wrap(ty: &Ident, shape: &Shape<'_>) -> TokenStream {
    match shape {
        Shape::Named(field, _) => quote!(|v| #ty { #field: v }),
        _ => quote!(#ty),
    }
}

/// The body of `parser_with_help` for a one-field type that is not a switch:
/// the probe prelude, the head, its completion, the parse and the arity.
#[allow(clippy::too_many_arguments)]
fn field_chain(
    ty: &Ident,
    shape: &Shape<'_>,
    field: &Type,
    value: &Type,
    arity: Arity,
    completion: &Completion,
    name: &LitStr,
    env: Option<&LitStr>,
    head: impl Fn(&TokenStream, TokenStream) -> TokenStream,
) -> TokenStream {
    let string = quote!(::std::string::String);
    // A path value keeps bpaf's own `OsString` conversion, which needs no
    // UTF-8, unless a completer has to see the word as a string.
    let os_head = is_os(value) && !matches!(completion, Completion::With(_));
    let parsed_as = if os_head {
        quote!(#value)
    } else {
        string.clone()
    };
    let probe = quote!((&::hicli::__private::Probe::<#value>::NEW));
    let complete_probe = if matches!(completion, Completion::None) && !os_head {
        quote!(let __complete = #probe.completer();)
    } else {
        TokenStream::new()
    };
    let h = head(&parsed_as, quote!(__help));
    let h = match completion {
        Completion::Files => quote!(::hicli::complete_files(#h)),
        Completion::Dirs => quote!(::hicli::complete_dirs(#h)),
        Completion::None if os_head => h,
        Completion::None => quote!(#h.complete(move |w: &#string| __complete(w.as_str()))),
        Completion::With(f) => quote!(#h.complete(|w: &#string| #f(w.as_str()))),
    };
    let map = wrap(ty, shape);
    let body = match arity {
        Arity::One => {
            let of = quote!((&::hicli::__private::FromStrOf::<#ty, #field>(#map)));
            let s = field.span();
            let closure = if os_head {
                let p = quote!(#field);
                parse_closure(s, &p, quote_spanned!(s=> #of.parse_os(w)), name, env)
            } else {
                let parse = quote_spanned!(s=> #of.parse_word(w.as_str()));
                parse_closure(s, &string, parse, name, env)
            };
            quote_spanned! {s=> #h.parse(#closure) }
        }
        Arity::Optional | Arity::Many => {
            let parsed = if os_head {
                h
            } else {
                let s = value.span();
                let closure = parse_closure(
                    s,
                    &string,
                    quote_spanned!(s=> w.parse::<#value>()),
                    name,
                    env,
                );
                let parse = quote_spanned! {s=> .parse(#closure) };
                quote!(#h #parse)
            };
            if arity == Arity::Optional {
                quote!(#parsed.optional().map(#map))
            } else {
                quote!(#parsed.many().map(#map))
            }
        }
    };
    quote! {
        #[allow(unused_imports)]
        use ::hicli::bpaf::Parser as _;
        #[allow(unused_imports)]
        use ::hicli::__private::{
            IsChoice as _, NotChoice as _, SelfFromStr as _, FieldFromStr as _, SelfFromOs as _,
            FieldFromOs as _,
        };
        let __help = #probe.help_row(help);
        #complete_probe
        #body
    }
}

/// The argument's spelling, as a parse error names it: a positional's
/// metavar, else `--long`, else `-s`.
fn spelling(args: &ArgArgs, ty: &Ident) -> String {
    if let Some(p) = &args.positional {
        return p
            .value
            .as_ref()
            .map_or_else(|| screaming_kebab(ty), LitStr::value);
    }
    if let Some(l) = &args.long {
        return format!(
            "--{}",
            l.value.as_ref().map_or_else(|| kebab(ty), LitStr::value)
        );
    }
    let s = args
        .short
        .as_ref()
        .expect("spelled: positional, long or short");
    format!("-{}", s.value.value())
}

/// Is `ty` a path value bpaf reads without UTF-8: `PathBuf` or `OsString`?
fn is_os(ty: &Type) -> bool {
    is_named(ty, "PathBuf") || is_named(ty, "OsString")
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
        let got = expansion("pub struct ShareOpts { #[bpaf(long)] a: bool, b: bool }");
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
    fn an_unknown_key_lists_every_key() {
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
            rejection("#[arg(short)]\n/// x\npub struct Id(bool);"),
            UNKNOWN_ARG
        );
        assert_eq!(
            rejection("#[arg(metavar)]\npub struct Id(String);"),
            UNKNOWN_ARG
        );
    }

    #[test]
    fn a_key_given_twice_is_refused() {
        assert_eq!(
            rejection("#[arg(complete = f, complete = g)]\npub struct Id(String);"),
            "`complete` is given twice"
        );
        assert_eq!(
            rejection("/// x\n#[arg(long, long = \"y\", metavar = \"M\")]\npub struct Id(String);"),
            "`long` is given twice"
        );
        assert_eq!(
            rejection("/// x\n#[arg(positional, positional)]\npub struct Id(String);"),
            "`positional` is given twice"
        );
        assert_eq!(
            rejection("/// x\n#[arg(short = 'a', short = 'b')]\npub struct Id(bool);"),
            "`short` is given twice"
        );
        assert_eq!(
            rejection(
                "/// x\n#[arg(long, metavar = \"A\", metavar = \"B\")]\npub struct Id(String);"
            ),
            "`metavar` is given twice"
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
            rejection(
                "#[arg(complete = f)]\npub struct Ref { #[bpaf(positional(\"DIR\"))] dir: String, name: String }"
            ),
            COMPLETE_NEEDS_ONE_FIELD
        );
        assert_eq!(
            rejection("#[arg(complete = f)]\npub struct Ref(String, String);"),
            COMPLETE_NEEDS_ONE_FIELD
        );
    }

    // --- composed mode -----------------------------------------------------------

    /// The two items a composed struct expands into, with `body` as the
    /// constructor's composition.
    /// `plain` are the types of the fields that are not `Option`: the
    /// composite's `OPTIONAL` is the AND of theirs.
    fn composed_items(
        vis: TokenStream,
        ty: &str,
        ctor: &str,
        plain: &[TokenStream],
        body: TokenStream,
    ) -> String {
        composed_with_checks(vis, ty, ctor, plain, body, TokenStream::new())
    }

    /// The same, with `checks` — the `Option` fields' assertions — between
    /// the constructor and the impl.
    fn composed_with_checks(
        vis: TokenStream,
        ty: &str,
        ctor: &str,
        plain: &[TokenStream],
        body: TokenStream,
        checks: TokenStream,
    ) -> String {
        let plain = plain.iter().map(|t| quote!(<#t as ::hicli::Arg>::OPTIONAL));
        let ty: Ident = syn::parse_str(ty).expect("an ident");
        let ctor: Ident = syn::parse_str(ctor).expect("an ident");
        let doc = format!("The parser `#[derive(Arg)]` wrote for [`{ty}`].");
        quote! {
            #[doc = #doc]
            #[doc(hidden)]
            #vis fn #ctor() -> impl ::hicli::bpaf::Parser<#ty> {
                #[allow(unused_imports)]
                use ::hicli::bpaf::Parser as _;
                #body
            }
            #checks
            #[automatically_derived]
            impl ::hicli::Arg for #ty {
                const OPTIONAL: bool = true #(&& #plain)*;
                fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                    #ctor()
                }
            }
        }
        .to_string()
    }

    /// The assertion beside an `Option<#t>`, with the message for `text`.
    fn check(t: TokenStream, text: &str) -> TokenStream {
        let message = format!(
            "`{text}` succeeds when its argument is absent, so `Option<{text}>` would always be \
             `Some`: use `{text}` as it is, or make `{text}` required and write `Option<{text}>` \
             where it may be left out"
        );
        quote!(
            const _: () = ::core::assert!(!<#t as ::hicli::Arg>::OPTIONAL, #message);
        )
    }

    #[test]
    fn composed_option_field() {
        let got = expansion(
            "pub struct Search { limit: Option<Limit>, key: std::option::Option<a::Key> }",
        );
        let c1 = check(quote!(Limit), "Limit");
        let c2 = check(quote!(a::Key), "a::Key");
        let want = composed_with_checks(
            quote!(pub),
            "Search",
            "search",
            &[],
            quote! {
                let limit = <Limit as ::hicli::Arg>::parser().optional();
                let key = <a::Key as ::hicli::Arg>::parser().optional();
                ::hicli::bpaf::construct!(Search { limit, key })
            },
            quote!(#c1 #c2),
        );
        assert_eq!(got, want);
    }

    #[test]
    fn composed_option_field_with_doc_override() {
        let got = expansion("pub struct Search {\n/// At most this many\nlimit: Option<Limit> }");
        let c = check(quote!(Limit), "Limit");
        let want = composed_with_checks(
            quote!(pub),
            "Search",
            "search",
            &[],
            quote! {
                let limit = <Limit as ::hicli::ArgHelp>::parser_with_help("At most this many").optional();
                ::hicli::bpaf::construct!(Search { limit })
            },
            c,
        );
        assert_eq!(got, want);
    }

    #[test]
    fn a_composite_is_optional_when_every_field_is() {
        // An `Option` field always succeeds, so only the plain fields count.
        let got = expansion("pub struct S { a: Option<Limit>, b: Query }");
        let c = check(quote!(Limit), "Limit");
        let want = composed_with_checks(
            quote!(pub),
            "S",
            "s",
            &[quote!(Query)],
            quote! {
                let a = <Limit as ::hicli::Arg>::parser().optional();
                let b = <Query as ::hicli::Arg>::parser();
                ::hicli::bpaf::construct!(S { a, b })
            },
            c,
        );
        assert_eq!(got, want);
    }

    #[test]
    fn optional_twice_and_a_cfg_on_a_field_are_refused() {
        assert_eq!(
            rejection("pub struct S { a: Option<std::option::Option<Limit>> }"),
            DOUBLE_OPTION
        );
        assert_eq!(
            rejection("pub struct S { #[cfg(unix)] a: Limit }"),
            COMPOSED_CFG
        );
        assert_eq!(
            rejection("pub struct S { #[cfg_attr(unix, allow(dead_code))] a: Limit }"),
            COMPOSED_CFG
        );
    }

    #[test]
    fn a_generic_composite_is_refused_so_no_check_needs_skipping() {
        // A const item cannot name an outer type's parameters, so a generic
        // composite's `Option<T>` field could not be checked. The derive
        // refuses generics outright (A4), which is what makes the per-field
        // assertion always writable.
        assert_eq!(rejection("pub struct S<T> { a: Option<T> }"), GENERIC);
    }

    #[test]
    fn a_named_struct_without_bpaf_attributes_composes() {
        let got = expansion("pub struct ShareOpts { a: Apply, b: Yes }");
        let want = composed_items(
            quote!(pub),
            "ShareOpts",
            "share_opts",
            &[quote!(Apply), quote!(Yes)],
            quote! {
                let a = <Apply as ::hicli::Arg>::parser();
                let b = <Yes as ::hicli::Arg>::parser();
                ::hicli::bpaf::construct!(ShareOpts { a, b })
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn composed_arg_fields() {
        let got = expansion("struct Listing { sort: SortOpt, r#type: crate::Kind, query: Query }");
        let want = composed_items(
            quote!(),
            "Listing",
            "listing",
            &[quote!(SortOpt), quote!(crate::Kind), quote!(Query)],
            quote! {
                let sort = <SortOpt as ::hicli::Arg>::parser();
                let r#type = <crate::Kind as ::hicli::Arg>::parser();
                let query = <Query as ::hicli::Arg>::parser();
                ::hicli::bpaf::construct!(Listing { sort, r#type, query })
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn composed_field_doc_overrides_through_arg_help() {
        let got = expansion(
            "pub struct Listing {\n\
             /// Order for\n/// this listing\n///\n/// Developer prose.\n\
             by: SortOpt,\n\
             ///\n///   \n\
             query: Query }",
        );
        let want = composed_items(
            quote!(pub),
            "Listing",
            "listing",
            &[quote!(SortOpt), quote!(Query)],
            quote! {
                let by = <SortOpt as ::hicli::ArgHelp>::parser_with_help("Order for this listing");
                let query = <Query as ::hicli::Arg>::parser();
                ::hicli::bpaf::construct!(Listing { by, query })
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn composed_doc_is_a_group_heading() {
        let got = expansion(
            "/// Global options\n/// for playr\n///\n/// Rustdoc only.\n\
             pub struct Globals { node: NodeOpt }",
        );
        let want = composed_items(
            quote!(pub),
            "Globals",
            "globals",
            &[quote!(NodeOpt)],
            quote! {
                let node = <NodeOpt as ::hicli::Arg>::parser();
                ::hicli::bpaf::construct!(Globals { node }).group_help("Global options for playr")
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn composed_rejections() {
        assert_eq!(rejection("pub struct S {}"), COMPOSED_EMPTY);
        assert_eq!(
            rejection("#[arg(metavar = \"M\")]\npub struct S { a: A }"),
            METAVAR_ALONE
        );
        assert_eq!(
            rejection("#[arg(complete = f)]\npub struct Ref { dir: Dir, name: Name }"),
            COMPOSED_COMPLETE
        );
        assert_eq!(
            rejection("#[arg(complete = f)]\npub struct Ref { dir: Dir }"),
            COMPOSED_COMPLETE
        );
    }

    #[test]
    fn arg_on_a_field_is_refused_in_every_mode() {
        for src in [
            "pub struct S { #[arg(long)] a: A }",
            "pub struct S { #[arg] a: A }",
            "/// x\n#[arg(positional)]\npub struct S(#[arg(positional)] String);",
            "pub struct S { #[bpaf(long)] #[arg(long)] a: bool }",
            "pub enum E { A(#[arg(long)] String) }",
            "/// x\n#[arg(positional)]\npub enum E { #[arg(long)] A, B }",
            "pub enum E { #[arg] A }",
        ] {
            assert_eq!(rejection(src), FIELD_ARG, "{src}");
        }
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

    #[test]
    fn metavar_without_a_spelling_is_refused() {
        assert_eq!(
            rejection("#[arg(metavar = \"M\")]\npub struct Id(String);"),
            METAVAR_ALONE
        );
    }

    // --- spelled mode: golden expansions ---------------------------------------

    /// The three items of a required type, with `chain` as
    /// `parser_with_help`'s body.
    fn three(vis: TokenStream, ty: &str, ctor: &str, help: &str, chain: TokenStream) -> String {
        items(vis, ty, ctor, help, false, chain)
    }

    /// The three items of a type that succeeds without its argument: its
    /// `Arg` impl says `OPTIONAL`.
    fn three_optional(
        vis: TokenStream,
        ty: &str,
        ctor: &str,
        help: &str,
        chain: TokenStream,
    ) -> String {
        items(vis, ty, ctor, help, true, chain)
    }

    /// The three items.
    fn items(
        vis: TokenStream,
        ty: &str,
        ctor: &str,
        help: &str,
        optional: bool,
        chain: TokenStream,
    ) -> String {
        let optional = optional.then(|| {
            quote!(
                const OPTIONAL: bool = true;
            )
        });
        let ty: Ident = syn::parse_str(ty).expect("an ident");
        let ctor: Ident = syn::parse_str(ctor).expect("an ident");
        let doc = format!("The parser `#[derive(Arg)]` wrote for [`{ty}`].");
        quote! {
            #[doc = #doc]
            #[doc(hidden)]
            #vis fn #ctor() -> impl ::hicli::bpaf::Parser<#ty> {
                <#ty as ::hicli::ArgHelp>::parser_with_help(#help)
            }
            #[automatically_derived]
            impl ::hicli::Arg for #ty {
                #optional
                fn parser() -> impl ::hicli::bpaf::Parser<Self> {
                    #ctor()
                }
            }
            #[automatically_derived]
            impl ::hicli::ArgHelp for #ty {
                fn parser_with_help(help: &'static str) -> impl ::hicli::bpaf::Parser<Self> {
                    #[allow(unused_imports)]
                    use ::hicli::bpaf::Parser as _;
                    #chain
                }
            }
        }
        .to_string()
    }

    /// The probe prelude for `T`: the help row, and the completer when `complete`.
    fn probe(t: TokenStream, complete: bool) -> TokenStream {
        let c = if complete {
            quote!(let __complete = (&::hicli::__private::Probe::<#t>::NEW).completer();)
        } else {
            TokenStream::new()
        };
        quote! {
            #[allow(unused_imports)]
            use ::hicli::__private::{
                IsChoice as _, NotChoice as _, SelfFromStr as _, FieldFromStr as _,
                SelfFromOs as _, FieldFromOs as _,
            };
            let __help = (&::hicli::__private::Probe::<#t>::NEW).help_row(help);
            #c
        }
    }

    #[test]
    fn spelled_positional_string() {
        let got = expansion(
            "/// The event's title\n#[arg(positional = \"TITLE\")]\npub struct Title(String);",
        );
        let p = probe(quote!(String), true);
        let want = three(
            quote!(pub),
            "Title",
            "title",
            "The event's title",
            quote! {
                #p
                ::hicli::bpaf::positional::<::std::string::String>("TITLE").help(__help)
                    .complete(move |w: &::std::string::String| __complete(w.as_str()))
                    .parse(|w: ::std::string::String|
                        (&::hicli::__private::FromStrOf::<Title, String>(Title))
                            .parse_word(w.as_str())
                            .map_err(|e| ::hicli::__private::named("TITLE", e)))
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_positional_choice_newtype() {
        let got = expansion(
            "/// A shell\n#[arg(positional = \"SHELL\")]\npub struct ShellArg(pub hicli::Shell);",
        );
        let p = probe(quote!(hicli::Shell), true);
        let want = three(
            quote!(pub),
            "ShellArg",
            "shell_arg",
            "A shell",
            quote! {
                #p
                ::hicli::bpaf::positional::<::std::string::String>("SHELL").help(__help)
                    .complete(move |w: &::std::string::String| __complete(w.as_str()))
                    .parse(|w: ::std::string::String|
                        (&::hicli::__private::FromStrOf::<ShellArg, hicli::Shell>(ShellArg))
                            .parse_word(w.as_str())
                            .map_err(|e| ::hicli::__private::named("SHELL", e)))
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_optional_choice_long_with_short() {
        let got = expansion(
            "/// Sort the listing\n/// by this field\n///\n/// Developer prose.\n\
             #[arg(long = \"sort\", short = 's', metavar = \"FIELD\")]\n\
             pub struct SortOpt(pub Option<SortField>);",
        );
        let p = probe(quote!(SortField), true);
        let want = three_optional(
            quote!(pub),
            "SortOpt",
            "sort_opt",
            "Sort the listing by this field",
            quote! {
                #p
                ::hicli::bpaf::long("sort").short('s').help(__help)
                    .argument::<::std::string::String>("FIELD")
                    .complete(move |w: &::std::string::String| __complete(w.as_str()))
                    .parse(|w: ::std::string::String|
                        w.parse::<SortField>().map_err(|e| ::hicli::__private::named("--sort", e)))
                    .optional()
                    .map(SortOpt)
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_one_value_named_field_parses_through_the_probe() {
        let got = expansion(
            "/// The port\n#[arg(long, metavar = \"PORT\")]\npub struct Port { pub port: u16 }",
        );
        let p = probe(quote!(u16), true);
        let want = three(
            quote!(pub),
            "Port",
            "port",
            "The port",
            quote! {
                #p
                ::hicli::bpaf::long("port").help(__help)
                    .argument::<::std::string::String>("PORT")
                    .complete(move |w: &::std::string::String| __complete(w.as_str()))
                    .parse(|w: ::std::string::String|
                        (&::hicli::__private::FromStrOf::<Port, u16>(|v| Port { port: v }))
                            .parse_word(w.as_str())
                            .map_err(|e| ::hicli::__private::named("--port", e)))
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_whole_struct_parses_through_its_own_from_str() {
        let got = expansion(
            "/// A file\n#[arg(positional = \"FILE\", complete = files)]\n\
             pub struct Upload { pub path: PathBuf, pub zip: bool }",
        );
        let want = three(
            quote!(pub),
            "Upload",
            "upload",
            "A file",
            quote! {
                ::hicli::complete_files(
                    ::hicli::bpaf::positional::<::std::string::String>("FILE").help(help)
                )
                .parse(|w: ::std::string::String|
                    w.parse::<Self>().map_err(|e| ::hicli::__private::named("FILE", e)))
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelling_names_the_argument() {
        let args = |src: &str| {
            let input: DeriveInput = syn::parse_str(src).expect("the type parses");
            let (args, _) = read_attrs(&input.attrs).expect("reads");
            spelling(&args, &input.ident)
        };
        assert_eq!(
            args("#[arg(long = \"sort\", short = 's', metavar = \"F\")] struct S(u8);"),
            "--sort"
        );
        assert_eq!(
            args("#[arg(short = 'n', metavar = \"N\")] struct N(u8);"),
            "-n"
        );
        assert_eq!(args("#[arg(positional)] struct SortBy(u8);"), "SORT-BY");
        assert_eq!(
            args("#[arg(long, metavar = \"M\")] struct SortBy(u8);"),
            "--sort-by"
        );
        assert_eq!(args("#[arg(positional = \"KEY\")] struct K(u8);"), "KEY");
    }

    #[test]
    fn spelled_switch() {
        let got = expansion(
            "/// Sort descending\n#[arg(long = \"descending\", short = 'd')]\nstruct Descending(bool);",
        );
        let want = three_optional(
            quote!(),
            "Descending",
            "descending",
            "Sort descending",
            quote! {
                ::hicli::bpaf::long("descending").short('d').help(help).switch().map(Descending)
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_vec_positional_with_a_completer() {
        let got = expansion(
            "/// Tags\n#[arg(positional = \"TAG\", complete = tags)]\npub struct Tags(Vec<String>);",
        );
        let p = probe(quote!(String), false);
        let want = three_optional(
            quote!(pub),
            "Tags",
            "tags",
            "Tags",
            quote! {
                #p
                ::hicli::bpaf::positional::<::std::string::String>("TAG").help(__help)
                    .complete(|w: &::std::string::String| tags(w.as_str()))
                    .parse(|w: ::std::string::String|
                        w.parse::<String>().map_err(|e| ::hicli::__private::named("TAG", e)))
                    .many()
                    .map(Tags)
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_path_with_files_is_typed() {
        let got = expansion(
            "/// A file\n#[arg(positional = \"FILE\", complete = files)]\npub struct Input(std::path::PathBuf);",
        );
        let p = probe(quote!(std::path::PathBuf), false);
        let want = three(
            quote!(pub),
            "Input",
            "input",
            "A file",
            quote! {
                #p
                ::hicli::complete_files(
                    ::hicli::bpaf::positional::<std::path::PathBuf>("FILE").help(__help)
                )
                .parse(|w: std::path::PathBuf|
                    (&::hicli::__private::FromStrOf::<Input, std::path::PathBuf>(Input))
                        .parse_os(w)
                        .map_err(|e| ::hicli::__private::named("FILE", e)))
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_path_without_a_completer_is_typed() {
        let got = expansion(
            "/// Where to write\n#[arg(long = \"out\", metavar = \"DIR\")]\npub struct Out(PathBuf);",
        );
        let p = probe(quote!(PathBuf), false);
        let want = three(
            quote!(pub),
            "Out",
            "out",
            "Where to write",
            quote! {
                #p
                ::hicli::bpaf::long("out").help(__help).argument::<PathBuf>("DIR")
                    .parse(|w: PathBuf|
                        (&::hicli::__private::FromStrOf::<Out, PathBuf>(Out))
                            .parse_os(w)
                            .map_err(|e| ::hicli::__private::named("--out", e)))
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_optional_path_is_typed_and_unparsed() {
        let got = expansion(
            "/// Where to write\n#[arg(long = \"out\", metavar = \"DIR\")]\npub struct Out(Option<PathBuf>);",
        );
        let p = probe(quote!(PathBuf), false);
        let want = three_optional(
            quote!(pub),
            "Out",
            "out",
            "Where to write",
            quote! {
                #p
                ::hicli::bpaf::long("out").help(__help).argument::<PathBuf>("DIR")
                    .optional()
                    .map(Out)
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_self_choice_enum() {
        let got = expansion(
            "/// What to sort by\n#[arg(positional = \"KEY\")]\npub enum SortKey { Bpm, Title }",
        );
        let want = three(
            quote!(pub),
            "SortKey",
            "sort_key",
            "What to sort by",
            quote! {
                ::hicli::__private::choice::<Self, _>("KEY", help, |help|
                    ::hicli::bpaf::positional::<::std::string::String>("KEY").help(help)
                )
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_self_choice_enum_option() {
        let got = expansion(
            "/// What to sort by\n#[arg(long = \"by\", metavar = \"KEY\")]\npub enum SortKey { Bpm, Title }",
        );
        let want = three(
            quote!(pub),
            "SortKey",
            "sort_key",
            "What to sort by",
            quote! {
                ::hicli::__private::choice::<Self, _>("--by", help, |help|
                    ::hicli::bpaf::long("by").help(help).argument::<::std::string::String>("KEY")
                )
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn spelled_named_field() {
        let got = expansion(
            "/// The node\n#[arg(long, metavar = \"PORT\", complete = known_ports)]\npub struct Port { pub port: u16 }",
        );
        let p = probe(quote!(u16), false);
        let want = three(
            quote!(pub),
            "Port",
            "port",
            "The node",
            quote! {
                #p
                ::hicli::bpaf::long("port").help(__help)
                    .argument::<::std::string::String>("PORT")
                    .complete(|w: &::std::string::String| known_ports(w.as_str()))
                    .parse(|w: ::std::string::String|
                        (&::hicli::__private::FromStrOf::<Port, u16>(|v| Port { port: v }))
                            .parse_word(w.as_str())
                            .map_err(|e| ::hicli::__private::named("--port", e)))
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn bare_long_and_positional_default_from_the_ident() {
        let long = expansion("/// x\n#[arg(long, metavar = \"M\")]\npub struct SortBy(String);");
        assert!(
            long.contains(":: hicli :: bpaf :: long (\"sort-by\")"),
            "{long}"
        );
        let pos = expansion("/// x\n#[arg(positional)]\npub struct SortBy(String);");
        assert!(
            pos.contains("positional :: < :: std :: string :: String > (\"SORT-BY\")"),
            "{pos}"
        );
        let short = expansion("/// x\n#[arg(short = 'v')]\npub struct Verbose(bool);");
        assert!(
            short.contains(":: hicli :: bpaf :: short ('v') . help (help) . switch ()"),
            "{short}"
        );
    }

    #[test]
    fn env_follows_the_long_name() {
        let got = expansion(
            "/// The node to talk to\n\
             #[arg(long = \"node\", metavar = \"HOST\", env = \"PLAYR_NODE\")]\n\
             pub struct Globals(pub Option<String>);",
        );
        let p = probe(quote!(String), true);
        let want = three_optional(
            quote!(pub),
            "Globals",
            "globals",
            "The node to talk to",
            quote! {
                #p
                ::hicli::bpaf::long("node").env("PLAYR_NODE").help(__help)
                    .argument::<::std::string::String>("HOST")
                    .complete(move |w: &::std::string::String| __complete(w.as_str()))
                    .parse(|w: ::std::string::String| {
                        let __env = ::hicli::__private::from_env(
                            "PLAYR_NODE",
                            ::core::convert::AsRef::<::std::ffi::OsStr>::as_ref(&w),
                        );
                        w.parse::<String>().map_err(|e|
                            ::hicli::__private::named_env("--node", "PLAYR_NODE", __env, e))
                    })
                    .optional()
                    .map(Globals)
            },
        );
        assert_eq!(got, want);
    }

    #[test]
    fn env_follows_the_short_name() {
        let got = expansion(
            "/// What to sort by\n#[arg(short = 'k', metavar = \"KEY\", env = \"KEY\")]\n\
             pub enum SortKey { Bpm, Title }",
        );
        let want = three(
            quote!(pub),
            "SortKey",
            "sort_key",
            "What to sort by",
            quote! {
                ::hicli::__private::choice_env::<Self, _>("-k", "KEY", help, |help|
                    ::hicli::bpaf::short('k').env("KEY").help(help)
                        .argument::<::std::string::String>("KEY")
                )
            },
        );
        assert_eq!(got, want);
        let both = expansion(
            "/// x\n#[arg(long = \"a\", short = 'a', metavar = \"M\", env = \"A\")]\n\
             pub struct A(String);",
        );
        assert!(
            both.contains(":: hicli :: bpaf :: long (\"a\") . short ('a') . env (\"A\") . help"),
            "{both}"
        );
        // A one-value field: the env check comes before the probe's parse.
        assert!(
            both.contains(
                &quote! {
                    |w: ::std::string::String| {
                        let __env = ::hicli::__private::from_env(
                            "A",
                            ::core::convert::AsRef::<::std::ffi::OsStr>::as_ref(&w),
                        );
                        (&::hicli::__private::FromStrOf::<A, String>(A))
                            .parse_word(w.as_str())
                            .map_err(|e| ::hicli::__private::named_env("--a", "A", __env, e))
                    }
                }
                .to_string()
            ),
            "{both}"
        );
        // A path parsed as bpaf read it: the check comes before `parse_os` moves it.
        let os = expansion(
            "/// x\n#[arg(long = \"out\", metavar = \"DIR\", env = \"OUT\")]\npub struct Out(PathBuf);",
        );
        assert!(
            os.contains(
                &quote! {
                    |w: PathBuf| {
                        let __env = ::hicli::__private::from_env(
                            "OUT",
                            ::core::convert::AsRef::<::std::ffi::OsStr>::as_ref(&w),
                        );
                        (&::hicli::__private::FromStrOf::<Out, PathBuf>(Out))
                            .parse_os(w)
                            .map_err(|e| ::hicli::__private::named_env("--out", "OUT", __env, e))
                    }
                }
                .to_string()
            ),
            "{os}"
        );
        // A struct parsed whole names it the same way.
        let whole = expansion(
            "/// x\n#[arg(long = \"r\", metavar = \"R\", env = \"R\")]\npub struct R { a: String, b: String }",
        );
        assert!(
            whole.contains(
                "w . parse :: < Self > () . map_err (| e | :: hicli :: __private :: named_env (\"--r\" , \"R\" , __env , e))"
            ),
            "{whole}"
        );
    }

    #[test]
    fn each_env_rejection_is_raised() {
        assert_eq!(
            rejection("/// x\n#[arg(positional, env = \"X\")]\npub struct Id(String);"),
            ENV_ON_POSITIONAL
        );
        assert_eq!(
            rejection("/// x\n#[arg(long, env = \"X\")]\npub struct On(bool);"),
            ENV_ON_SWITCH
        );
        assert_eq!(
            rejection("#[arg(env = \"X\")]\npub struct Id(String);"),
            ENV_ALONE
        );
        assert_eq!(
            rejection("#[arg(env = \"X\")]\npub struct S { a: A }"),
            ENV_ALONE
        );
        for bad in ["\"\"", "\"A=B\"", "\"A B\""] {
            assert_eq!(
                rejection(&format!(
                    "/// x\n#[arg(long, metavar = \"M\", env = {bad})]\npub struct Id(String);"
                )),
                BAD_ENV,
                "{bad}"
            );
        }
        assert_eq!(
            rejection("/// x\n#[arg(env)]\npub struct Id(String);"),
            UNKNOWN_ARG
        );
        assert_eq!(
            rejection(
                "/// x\n#[arg(long, metavar = \"M\", env = \"A\", env = \"B\")]\npub struct Id(String);"
            ),
            "`env` is given twice"
        );
    }

    // --- spelled mode: rejections ----------------------------------------------

    #[test]
    fn s1_a_spelled_type_needs_a_doc_comment() {
        assert_eq!(
            rejection("#[arg(positional)]\npub struct Id(String);"),
            NEEDS_DOC
        );
        assert_eq!(
            rejection("///\n///   \n#[arg(positional)]\npub struct Id(String);"),
            NEEDS_DOC
        );
    }

    #[test]
    fn s2_positional_with_long_or_short() {
        assert_eq!(
            rejection("/// x\n#[arg(positional, long)]\npub struct Id(String);"),
            POSITIONAL_AND_NAMED
        );
        assert_eq!(
            rejection("/// x\n#[arg(short = 'i', positional)]\npub struct Id(String);"),
            POSITIONAL_AND_NAMED
        );
    }

    #[test]
    fn s3_a_positional_bool() {
        assert_eq!(
            rejection("/// x\n#[arg(positional)]\npub struct On(bool);"),
            POSITIONAL_BOOL
        );
    }

    #[test]
    fn s4_metavar_on_a_bool_or_a_positional() {
        assert_eq!(
            rejection("/// x\n#[arg(long, metavar = \"M\")]\npub struct On(bool);"),
            METAVAR_ON_BOOL
        );
        assert_eq!(
            rejection("/// x\n#[arg(positional, metavar = \"M\")]\npub struct Id(String);"),
            METAVAR_ON_POSITIONAL
        );
    }

    #[test]
    fn s5_an_option_needs_a_metavar() {
        assert_eq!(
            rejection("/// x\n#[arg(long)]\npub struct Id(String);"),
            NEEDS_METAVAR
        );
        assert_eq!(
            rejection("/// x\n#[arg(short = 'i')]\npub enum Mode { A, B }"),
            NEEDS_METAVAR
        );
    }

    #[test]
    fn s6_an_optional_or_repeated_switch() {
        assert_eq!(
            rejection("/// x\n#[arg(long)]\npub struct On(Option<bool>);"),
            OPTIONAL_SWITCH
        );
        assert_eq!(
            rejection("/// x\n#[arg(long)]\npub struct On(Vec<bool>);"),
            OPTIONAL_SWITCH
        );
    }

    #[test]
    fn s7_a_unit_struct_or_an_enum_with_data_is_another_shape() {
        assert_eq!(
            rejection("/// x\n#[arg(positional)]\npub struct R;"),
            SPELLED_SHAPE
        );
        assert_eq!(
            rejection("/// x\n#[arg(positional)]\npub enum R { A(String) }"),
            SPELLED_SHAPE
        );
    }

    #[test]
    fn s7_a_struct_with_several_fields_is_parsed_whole() {
        let whole = |meta: &str| {
            quote! {
                ::hicli::bpaf::positional::<::std::string::String>(#meta).help(help)
                    .parse(|w: ::std::string::String|
                        w.parse::<Self>().map_err(|e| ::hicli::__private::named(#meta, e)))
            }
        };
        assert_eq!(
            expansion("/// x\n#[arg(positional)]\npub struct R(String, String);"),
            three(quote!(pub), "R", "r", "x", whole("R"))
        );
        assert_eq!(
            expansion("/// x\n#[arg(positional)]\npub struct R { a: String, b: String }"),
            three(quote!(pub), "R", "r", "x", whole("R"))
        );
    }

    #[test]
    fn s8_bpaf_attributes_on_a_spelled_type() {
        assert_eq!(
            rejection(
                "/// x\n#[arg(positional)]\npub struct Id(#[bpaf(positional(\"ID\"))] String);"
            ),
            BPAF_ATTRS
        );
        assert_eq!(
            rejection("/// x\n#[bpaf(options)]\n#[arg(positional)]\npub struct Id(String);"),
            BPAF_ATTRS
        );
    }

    #[test]
    fn s9_a_bad_short_or_long() {
        for bad in ["'-'", "' '", "'\\t'"] {
            assert_eq!(
                rejection(&format!(
                    "/// x\n#[arg(short = {bad})]\npub struct On(bool);"
                )),
                BAD_SHORT,
                "{bad}"
            );
        }
        for bad in ["\"\"", "\"--x\"", "\"a b\""] {
            assert_eq!(
                rejection(&format!(
                    "/// x\n#[arg(long = {bad})]\npub struct On(bool);"
                )),
                BAD_LONG,
                "{bad}"
            );
        }
    }

    #[test]
    fn s11_complete_on_a_switch() {
        assert_eq!(
            rejection("/// x\n#[arg(long, complete = f)]\npub struct On(bool);"),
            COMPLETE_ON_BOOL
        );
    }
}
