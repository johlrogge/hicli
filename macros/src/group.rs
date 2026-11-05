//! Writing what a `#[group]` becomes: the module, verbatim, and one handle.
//!
//! The annotated module is re-emitted **untouched** — inner `#[command]` and
//! `#[group]` attributes included, for rustc to expand next — so the group is
//! ordinary Rust that rustfmt formats and rust-analyzer understands. Beside it,
//! in the *parent* scope, goes the handle: a hidden
//! `fn __hicli_<ident>() -> ::hicli::Command<Ctx>` that pushes one child
//! handle per discovered child and hands the vector to `hicli::group`.
//!
//! The handle reaches exactly **one level down** — `__hicli_event()` names
//! `event::__hicli_create()`, never `event::audience::__hicli_define()` —
//! which is what makes "every handle is `pub`" sufficient rather than merely
//! convenient.
//!
//! The children are pushed one statement at a time, rather than collected into
//! a `vec![…]`, so that each child's `#[cfg]` can ride on its own registration:
//! a statement takes an attribute, an element of a `vec!` does not.
//!
//! # The two forms
//!
//! A group's body is either **inline** (`#[group] mod event { … }`) or in a
//! **file** (`#[group] pub mod event;`). Both are the same attribute, and they
//! differ in exactly one thing: where the items come from. The inline form has
//! them already; the file form resolves the declaration to a path
//! (`crate::resolve`), reads it and parses it. From there, everything — the doc
//! split, the children, the context, the handle — is the same code.

use std::io;
use std::path::Path;

use proc_macro2::{Group, Span, TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Attribute, Error, Ident, Item, ItemMod, LitStr, Result, Token, Type};

use crate::ctx::{self, render, Ctx, Outline};
use crate::discover::{children_of, Child, ChildKind};
use crate::expand::metadata;
use crate::fs::Fs;
use crate::resolve::{resolve as resolve_file, show, Load};
use crate::shared::{
    cfg_attrs, dashed, handle_doc, handle_ident, parse_keys, path_attr, read_doc, unraw_name,
    CommandMeta, GROUP_NEEDS_DOC,
};

/// The message for an argument `#[group]` does not take (D10).
const UNKNOWN_ARG: &str = "#[group] takes `ctx = <Type>`, `args = <Type>`, `curried = <Type>`, \
                           `alias = \"...\"` (repeatable) and `hide`";

/// The message for `args` and `curried` on one group.
const ARGS_AND_CURRIED: &str = "`args` and `curried` cannot be combined yet: a group that \
                                receives its parent's argument cannot take another of its own \
                                (deferred)";

/// The message for `default` on a group.
const GROUP_DEFAULT: &str =
    "`default` goes on a #[command]: a group runs its default command, it cannot be one";

/// The message for `args` or `curried` written without its type.
fn needs_type(key: &Ident) -> String {
    format!("`{key}` on a group names a type: write `{key} = <Type>`")
}

/// The message for `#[group]` on anything but a module (D9).
const NOT_A_MODULE: &str = "#[group] goes on a module: #[group] pub mod event { … }";

/// The file a group was declared in — or, when it could not be found, every
/// path that was looked for (D6).
///
/// Only the file form needs it, and only the file form can fail without it, so
/// the failure carries its own explanation rather than leaving the diagnostic
/// to guess what the compiler said.
#[derive(Clone, Copy)]
pub(crate) enum Source<'a> {
    /// The declaring file, relative to the anchor it was found under.
    Found(&'a Path),
    /// The candidates, as the compiler spelled them, none of which opened.
    Unknown(&'a [String]),
}

/// What `#[group]` itself was given: `ctx = <Type>`, `args = <Type>`,
/// `curried = <Type>`, `alias = "…"` (repeatable) and `hide`, each optional.
#[derive(Default)]
pub(crate) struct GroupArgs {
    /// The context type, said out loud instead of lifted from a child.
    ctx: Option<Type>,
    /// Whether the group takes an argument, receives one, or neither.
    shape: Shape,
    /// The aliases and the visibility, read by the rules `#[command]` shares.
    meta: CommandMeta,
}

/// How a group relates to a curried argument (ADR-011).
#[derive(Default)]
pub(crate) enum Shape {
    /// An ordinary group: every child is a `Command<Ctx>`.
    #[default]
    Plain,
    /// `args = T`: the group parses a `T` and hands it to its `curried`
    /// children; the others are its fixed siblings.
    Args(Type),
    /// `curried = T`: the group receives its parent's `T` and hands it on, so
    /// every child receives it.
    Curried(Type),
}

impl GroupArgs {
    /// Whether `args = T` was given — what a parent `#[group]` reads.
    pub(crate) fn takes_args(&self) -> bool {
        matches!(self.shape, Shape::Args(_))
    }

    /// Whether `curried = T` was given — what a parent `#[group]` reads to
    /// sort this child into the ones that receive its argument.
    pub(crate) fn is_curried(&self) -> bool {
        matches!(self.shape, Shape::Curried(_))
    }
}

/// Read `= <Type>` after `key` into `slot`, refusing a repeat and a missing
/// type.
fn typed_key(slot: &mut Option<(Ident, Type)>, key: &Ident, input: ParseStream) -> Result<()> {
    if slot.is_some() {
        return Err(Error::new_spanned(key, format!("`{key}` is given twice")));
    }
    if !input.peek(Token![=]) {
        return Err(Error::new_spanned(key, needs_type(key)));
    }
    let _: Token![=] = input.parse()?;
    *slot = Some((key.clone(), input.parse()?));
    Ok(())
}

impl Parse for GroupArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut ctx: Option<Type> = None;
        let mut args: Option<(Ident, Type)> = None;
        let mut curried: Option<(Ident, Type)> = None;
        let meta = parse_keys(input, UNKNOWN_ARG, |key, input| {
            if key == "args" {
                typed_key(&mut args, key, input)?;
                return Ok(true);
            }
            if key == "curried" {
                typed_key(&mut curried, key, input)?;
                return Ok(true);
            }
            if key == "default" {
                return Err(Error::new_spanned(key, GROUP_DEFAULT));
            }
            if key != "ctx" {
                return Ok(false);
            }
            if ctx.is_some() {
                return Err(Error::new_spanned(key, "`ctx` is given twice"));
            }
            let _: Token![=] = input.parse()?;
            ctx = Some(input.parse()?);
            Ok(true)
        })?;
        let shape = match (args, curried) {
            (Some((key, _)), Some(_)) => return Err(Error::new_spanned(key, ARGS_AND_CURRIED)),
            (Some((_, ty)), None) => Shape::Args(ty),
            (None, Some((_, ty))) => Shape::Curried(ty),
            (None, None) => Shape::Plain,
        };
        Ok(GroupArgs { ctx, shape, meta })
    }
}

/// Refuse the children a group of this `shape` cannot have, each at its own
/// ident (ADR-011). Every such child is reported at once.
fn check_children(shape: &Shape, group: &Ident, children: &[Child]) -> Result<()> {
    let mut errors: Vec<Error> = Vec::new();
    let mut first_default: Option<&Ident> = None;
    for child in children {
        let kind = &child.kind;
        let ident = &child.ident;
        match shape {
            Shape::Plain if kind.is_curried() => {
                errors.push(Error::new_spanned(ident, curried_in_plain(ident, group)));
            }
            Shape::Curried(ty) if !kind.is_curried() => {
                errors.push(Error::new_spanned(
                    ident,
                    unmarked_in_curried(child, group, ty),
                ));
            }
            _ => {}
        }
        if kind.is_default() {
            match (shape, first_default) {
                (Shape::Args(_), _) => errors.push(Error::new_spanned(
                    ident,
                    default_with_argument(ident, group, "takes"),
                )),
                (Shape::Curried(_), _) => errors.push(Error::new_spanned(
                    ident,
                    default_with_argument(ident, group, "receives"),
                )),
                (Shape::Plain, Some(first)) => {
                    errors.push(Error::new_spanned(ident, second_default(group, first)));
                }
                (Shape::Plain, None) => first_default = Some(ident),
            }
        }
    }
    if let Shape::Args(ty) = shape {
        if !children.iter().any(|c| c.kind.is_curried()) {
            errors.push(Error::new_spanned(ty, nobody_curried(group, ty)));
        }
    }
    let mut errors = errors.into_iter();
    match errors.next() {
        None => Ok(()),
        Some(mut first) => {
            first.extend(errors);
            Err(first)
        }
    }
}

/// The message for a curried child of a group with no argument to give it.
fn curried_in_plain(child: &Ident, group: &Ident) -> String {
    format!(
        "`{child}` is curried, but group `{group}` has no argument to hand it: declare one with \
         #[group(args = <Type>)]"
    )
}

/// The message for a child of a `curried = T` group that does not take `T`.
fn unmarked_in_curried(child: &Child, group: &Ident, ty: &Type) -> String {
    let ident = &child.ident;
    let ty = render(ty);
    match &child.kind {
        ChildKind::Command { .. } => format!(
            "`{ident}` is in group `{group}`, which hands every child a `{ty}`: mark it \
             #[command(curried)] and take the `{ty}` as its first parameter"
        ),
        kind if kind.takes_args() => format!(
            "`{ident}` takes `args` of its own inside group `{group}`, which hands every child a \
             `{ty}`: a group with `args` inside a curried group is not supported yet"
        ),
        _ => format!(
            "`{ident}` is in group `{group}`, which hands every child a `{ty}`: mark it \
             #[group(curried = {ty})]"
        ),
    }
}

/// The message for a group's second `default` command.
fn second_default(group: &Ident, first: &Ident) -> String {
    format!("group `{group}` already has a default command, `{first}`: a group has at most one")
}

/// The message for a `default` command in a group that takes or receives an
/// argument.
fn default_with_argument(child: &Ident, group: &Ident, verb: &str) -> String {
    format!(
        "`{child}` cannot be a default command: group `{group}` {verb} an argument, so it has \
         no default command"
    )
}

/// The message for an `args` group none of whose children receives the
/// argument.
fn nobody_curried(group: &Ident, ty: &Type) -> String {
    format!(
        "group `{group}` takes `args = {}`, but none of its children receives it: mark the \
         commands that take it #[command(curried)]",
        render(ty)
    )
}

/// Expand `#[group]` on `item`, or explain why it cannot be a group.
///
/// `source` is the file the attribute was written in, when the compiler will
/// say — only the file form needs it.
pub(crate) fn expand(
    fs: &dyn Fs,
    source: Source<'_>,
    args: GroupArgs,
    item: Item,
) -> Result<TokenStream> {
    match item {
        Item::Mod(m) if m.content.is_some() => inline_form(args, m),
        Item::Mod(m) => file_form(fs, source, args, m),
        other => Err(Error::new_spanned(other, NOT_A_MODULE)),
    }
}

/// An inline `#[group] mod x { … }`: the module, then its handle.
fn inline_form(args: GroupArgs, item: ItemMod) -> Result<TokenStream> {
    let items = &item
        .content
        .as_ref()
        .expect("an inline module has a body")
        .1;
    let doc = read_doc(&item.attrs, &item.ident, GROUP_NEEDS_DOC)?;
    let name = dashed(&item.ident);
    args.meta.check_name(&name, "group")?;
    let children = children_of(items);
    if children.is_empty() {
        return Err(Error::new_spanned(
            &item.ident,
            no_children(&item.ident, "its body"),
        ));
    }
    check_no_file_children(&children)?;
    check_children(&args.shape, &item.ident, &children)?;
    let handle = emit_handle(&Handle {
        module: &item.ident,
        name,
        help: doc.help,
        details: doc.details,
        footer: doc.footer,
        aliases: args.meta.aliases,
        hide: args.meta.hide.is_some(),
        cfgs: cfg_attrs(&item.attrs),
        shape: args.shape,
        // An inline group has no file of its own, and its only file children
        // carry an explicit `#[path]` — so there is no file for the context
        // walk to follow.
        ctx: ctx::resolve(args.ctx, &children, None)?,
        children,
    });
    Ok(quote! {
        #item
        #handle
    })
}

/// Refuse a `#[group] mod b;` without a `#[path]` inside an inline group.
///
/// The child's own `#[group]` resolves its file against the *declaring file*,
/// because that is all `Span::local_file` can say; rustc resolves it against
/// the inline module's directory (`a/b.rs` for `mod a { mod b; }`). The two
/// disagree, so the child would read one file and rustc compile another — the
/// mistake is caught here, where it is written, rather than as a confusing
/// "cannot find the file". An explicit `#[path]` means the same thing to both,
/// so it is allowed. Every such child is reported at once.
fn check_no_file_children(children: &[Child]) -> Result<()> {
    let mut failed: Option<Error> = None;
    for child in children {
        if let ChildKind::Outlined { path: None, .. } = child.kind {
            let e = Error::new_spanned(&child.ident, file_in_inline(&child.ident));
            match &mut failed {
                Some(first) => first.combine(e),
                None => failed = Some(e),
            }
        }
    }
    failed.map_or(Ok(()), Err)
}

/// The message for a file group declared inside an inline group.
fn file_in_inline(ident: &Ident) -> String {
    format!(
        "`#[group] mod {ident};` inside an inline module resolves against the wrong directory; \
         make `{ident}` inline, or move the declaring module to its own file"
    )
}

/// A file module: `pub mod event;` plus the file it names.
///
/// The declaration comes back so that rustc reads exactly the bytes that were
/// read here: as written, where rustc's own rules find the file, and with a
/// synthesised `#[path]` everywhere else — including wherever the one question
/// a proc macro cannot answer (is this declaring file a crate root?) would
/// matter. A `#[path]` is avoided where it can be, because rustc treats a file
/// loaded through one as mod-rs. The handle comes back beside it, written from
/// the children found in the file.
fn file_form(
    fs: &dyn Fs,
    source: Source<'_>,
    args: GroupArgs,
    mut item: ItemMod,
) -> Result<TokenStream> {
    let doc = read_doc(&item.attrs, &item.ident, GROUP_NEEDS_DOC)?;
    let name = dashed(&item.ident);
    args.meta.check_name(&name, "group")?;
    let source = match source {
        Source::Found(file) => file,
        Source::Unknown(tried) => {
            return Err(Error::new(
                Span::call_site(),
                unknown_source(&item.ident, tried),
            ))
        }
    };

    let explicit = path_attr(&item.attrs);
    let found = resolve_file(fs, source, &unraw_name(&item.ident), explicit.as_deref())
        .map_err(|e| Error::new_spanned(&item.ident, not_found(&item.ident, &e.tried)))?;
    let file = show(&found.file);
    let text = fs
        .read_to_string(&found.file)
        .map_err(|e| Error::new_spanned(&item.ident, unreadable(&file, &e)))?;
    let parsed = syn::parse_file(&text)
        .map_err(|e| Error::new_spanned(&item.ident, unparsable(&item.ident, &file, &e)))?;

    let children = children_of(&parsed.items);
    if children.is_empty() {
        return Err(Error::new_spanned(
            &item.ident,
            no_children(&item.ident, &format!("`{file}`")),
        ));
    }
    check_children(&args.shape, &item.ident, &children)?;
    let ctx = ctx::resolve(
        args.ctx,
        &children,
        Some(&Outline::new(fs, found.file.clone())),
    )?;
    let handle = emit_handle(&Handle {
        module: &item.ident,
        name,
        help: doc.help,
        details: doc.details,
        footer: doc.footer,
        aliases: args.meta.aliases,
        hide: args.meta.hide.is_some(),
        cfgs: cfg_attrs(&item.attrs),
        shape: args.shape,
        ctx,
        children,
    });

    // rustc reads the bytes read here either because its own rules find them
    // or because a `#[path]` says so — and the `#[path]` is only added where
    // it is needed, because it makes the file mod-rs and would send a plain
    // `mod helper;` inside it to the wrong directory (#25). Never an absolute
    // path: `relative` is relative to the declaring file's directory, which is
    // what `#[path]` on an outlined module means (spike 0.4). An author who
    // wrote their own keeps it.
    if explicit.is_none() && found.load == Load::ByPath {
        let relative = &found.relative;
        item.attrs.push(syn::parse_quote!(#[path = #relative]));
    }
    Ok(quote! {
        #item
        #handle
    })
}

/// The message for a group with nothing in it (D2).
///
/// `hicli::group`'s runtime panic stays for hand-built trees; a group
/// *declared* as a module can be refused where it is written. `where` is the
/// place to put the missing command: a body, or a file, named.
fn no_children(ident: &Ident, where_: &str) -> String {
    format!(
        "group `{ident}` has no commands: add a #[command] function or a nested #[group] module \
         to {where_}"
    )
}

/// The message for a declaration whose file was not found (D3).
///
/// The note is the one limitation of the file form, said at the only moment it
/// can be mistaken for something else: a `#[group] mod b;` inside an inline
/// `mod a { … }` resolves against the declaring *file*, while rustc would
/// resolve against `a/`, and "cannot find the file" is exactly what that looks
/// like. (Inside an inline `#[group]` it is refused outright; inside a plain
/// inline `mod`, this note is the only warning.)
fn not_found(ident: &Ident, tried: &[String]) -> String {
    let tried: Vec<String> = tried.iter().map(|t| format!("`{t}`")).collect();
    format!(
        "cannot find the file for module `{ident}`: tried {}\n\
         note: #[group] on a file module must be at the top level of its file; inside an inline \
         `mod`, use an inline #[group] mod instead",
        tried.join(", ")
    )
}

/// The message for a file that would not parse as Rust (D4).
fn unparsable(ident: &Ident, file: &str, e: &Error) -> String {
    format!("cannot read module `{ident}` from `{file}`: {e}")
}

/// The message for a file that would not open (D5).
fn unreadable(file: &str, e: &io::Error) -> String {
    format!("cannot read `{file}`: {e}")
}

/// The message for a source file that could not be found (D6).
///
/// Under `--remap-path-prefix`, or a front end that implements neither
/// `Span::local_file` nor `Span::file`, there is no file to read and nothing to
/// guess from — so the answer is the form that needs no file at all. What the
/// compiler *did* say is named: a path that looks right but opens nowhere is
/// the only clue to which front end reported it, and where it thought it was.
fn unknown_source(ident: &Ident, tried: &[String]) -> String {
    let looked = if tried.is_empty() {
        "the path is remapped or synthetic".to_string()
    } else {
        let tried: Vec<String> = tried.iter().map(|t| format!("`{t}`")).collect();
        format!(
            "tried {} under the working directory and the manifest directory and its parents",
            tried.join(", ")
        )
    };
    format!(
        "hicli cannot locate the source file of this #[group] file module ({looked}); use an \
         inline #[group] mod {ident} {{ … }}"
    )
}

/// Everything a group's handle is written from.
///
/// Shared by both forms of `#[group]`: the inline module and the file module.
/// Only `module` and `children` differ between them, and neither changes what
/// the handle says.
pub(crate) struct Handle<'a> {
    /// The module the children live in: the path prefix of every child call.
    pub(crate) module: &'a Ident,
    /// The group's name on the command line.
    pub(crate) name: String,
    /// The one-line help: the doc comment's first line.
    pub(crate) help: String,
    /// The rest of the doc comment up to an examples heading, if any.
    pub(crate) details: Option<String>,
    /// The examples section, laid out for verbatim rendering, if any.
    pub(crate) footer: Option<String>,
    /// Every `alias = "…"`, in source order.
    pub(crate) aliases: Vec<LitStr>,
    /// Whether `hide` was given.
    pub(crate) hide: bool,
    /// The module's own `#[cfg]` / `#[cfg_attr]`, copied onto the handle so a
    /// configured-out group takes its registration with it.
    pub(crate) cfgs: Vec<Attribute>,
    /// Whether the group takes an argument, receives one, or neither.
    pub(crate) shape: Shape,
    /// The children, in source order — which is their order in `--help`.
    pub(crate) children: Vec<Child>,
    /// The type the handle names as its `Ctx`.
    pub(crate) ctx: Ctx,
}

/// Write the handle.
pub(crate) fn emit_handle(h: &Handle) -> TokenStream {
    // With a context the handle names it; without one it is generic over `Ctx`,
    // so a context-free group mounts in any app.
    let (generics, ctx_ty) = match &h.ctx {
        Ctx::Named(t) => {
            let t = t.as_ref();
            (quote!(), quote!(#t))
        }
        Ctx::Generic => (quote!(<Ctx: 'static>), quote!(Ctx)),
    };

    let handle = handle_ident(h.module);
    let doc = handle_doc(h.module);
    let cfgs = &h.cfgs;
    let metadata = metadata(
        h.details.as_deref(),
        h.footer.as_deref(),
        &h.aliases,
        h.hide,
        false,
    );
    let (ret, body) = match &h.shape {
        Shape::Plain => plain_body(h, &ctx_ty),
        Shape::Args(ty) => args_body(h, ty, &ctx_ty),
        Shape::Curried(ty) => curried_body(h, ty, &ctx_ty),
    };

    quote! {
        #[doc = #doc]
        #[doc(hidden)]
        #[allow(unreachable_pub)]
        #(#cfgs)*
        pub fn #handle #generics () -> #ret {
            #body
            #metadata
        }
    }
}

/// How a child's handle is pushed.
enum Push<'a> {
    /// Unspanned, as it always has been: an ordinary group's child.
    Plain,
    /// At the child's ident, so a type error is reported there.
    Spanned,
    /// At the child's ident, and through `hicli::__CurriedOver` — the path,
    /// generics included, is given — so a child that is not curried over the
    /// group's type is reported in the group's terms.
    CurriedOver(&'a TokenStream),
}

/// One `push` per child into `vec`, each under the child's own `#[cfg]`s.
///
/// Each child's handle is called *directly*, not through `IntoCommand`, so a
/// child whose context — or, in a curried group, whose curried type — is a
/// different type is an error at its own `push` rather than an unsatisfied
/// trait bound on the whole group. A spanned `push` is written at the child's
/// ident, module path included, so that is the only place the error points.
fn push(h: &Handle, c: &Child, vec: &Ident, how: &Push) -> TokenStream {
    let cfgs = &c.cfgs;
    let child = handle_ident(&c.ident);
    let span = c.ident.span();
    let call = match how {
        Push::Plain => {
            let module = h.module;
            quote!(#vec.push(#module::#child());)
        }
        Push::Spanned | Push::CurriedOver(_) => {
            let mut module = h.module.clone();
            module.set_span(span);
            let handle = quote_spanned!(span=> #module::#child());
            match how {
                Push::CurriedOver(over) => {
                    // Respanned whole, the group's type included: otherwise
                    // the call would also be labelled at the attribute.
                    let over = respan(over, span);
                    quote_spanned!(span=> #vec.push(#over::__node(#handle));)
                }
                _ => quote_spanned!(span=> #vec.push(#handle);),
            }
        }
    };
    quote! {
        #(#cfgs)*
        #call
    }
}

/// `tokens`, every one of them — inside groups too — at `span`.
fn respan(tokens: &TokenStream, span: Span) -> TokenStream {
    tokens
        .clone()
        .into_iter()
        .map(|tt| match tt {
            TokenTree::Group(g) => {
                let mut inner = Group::new(g.delimiter(), respan(&g.stream(), span));
                inner.set_span(span);
                TokenTree::Group(inner)
            }
            mut other => {
                other.set_span(span);
                other
            }
        })
        .collect()
}

/// `::hicli::__CurriedOver::<T, Ctx>`, for [`Push::CurriedOver`].
fn curried_over(ty: &Type, ctx_ty: &TokenStream) -> TokenStream {
    quote!(::hicli::__CurriedOver::<#ty, #ctx_ty>)
}

/// An ordinary group: every child is a `Command<Ctx>`, handed to
/// `hicli::group`.
fn plain_body(h: &Handle, ctx_ty: &TokenStream) -> (TokenStream, TokenStream) {
    let children = Ident::new("__children", Span::call_site());
    let pushes = h
        .children
        .iter()
        .map(|c| push(h, c, &children, &Push::Plain));
    let name = &h.name;
    let help = &h.help;
    // The element type is spelled out rather than interpolated, so its
    // closing `>>` is the same joint token it has always been.
    let body = quote! {
        // Every child may be configured out, and then nothing is pushed.
        #[allow(unused_mut)]
        let mut __children: ::std::vec::Vec<::hicli::Command<#ctx_ty>> =
            ::std::vec::Vec::new();
        #(#pushes)*
        ::hicli::group(#name, #help, __children)
    };
    (quote!(::hicli::Command<#ctx_ty>), body)
}

/// `args = T`: the fixed children and the bound (curried) ones go into two
/// vectors, and `hicli::group_args` parses a `T` in front of the bound ones.
fn args_body(h: &Handle, ty: &Type, ctx_ty: &TokenStream) -> (TokenStream, TokenStream) {
    let fixed = Ident::new("__fixed", Span::call_site());
    let bound = Ident::new("__bound", Span::call_site());
    let over = curried_over(ty, ctx_ty);
    let pushes: Vec<TokenStream> = h
        .children
        .iter()
        .map(|c| {
            if c.kind.is_curried() {
                push(h, c, &bound, &Push::CurriedOver(&over))
            } else {
                push(h, c, &fixed, &Push::Spanned)
            }
        })
        .collect();
    let name = &h.name;
    let help = &h.help;
    // Spanned at the type the author wrote in `args = …`, so a type that is
    // not an `Arg` is reported there.
    let parser = quote_spanned!(ty.span()=> <#ty as ::hicli::Arg>::parser);
    let body = quote! {
        #[allow(unused_mut)]
        let mut __fixed: ::std::vec::Vec<::hicli::Command<#ctx_ty>> =
            ::std::vec::Vec::new();
        #[allow(unused_mut)]
        let mut __bound: ::std::vec::Vec<::hicli::Curried<#ty, #ctx_ty>> =
            ::std::vec::Vec::new();
        #(#pushes)*
        ::hicli::group_args(#name, #help, #parser, __fixed, __bound)
    };
    (quote!(::hicli::Command<#ctx_ty>), body)
}

/// `curried = T`: every child receives the `T`, and so does the group — it is
/// a `Curried<T, Ctx>` itself, built by `hicli::group`.
fn curried_body(h: &Handle, ty: &Type, ctx_ty: &TokenStream) -> (TokenStream, TokenStream) {
    let children = Ident::new("__children", Span::call_site());
    let over = curried_over(ty, ctx_ty);
    let pushes: Vec<TokenStream> = h
        .children
        .iter()
        .map(|c| push(h, c, &children, &Push::CurriedOver(&over)))
        .collect();
    let name = &h.name;
    let help = &h.help;
    let body = quote! {
        #[allow(unused_mut)]
        let mut __children: ::std::vec::Vec<::hicli::Curried<#ty, #ctx_ty>> =
            ::std::vec::Vec::new();
        #(#pushes)*
        ::hicli::group(#name, #help, __children)
    };
    (quote!(::hicli::Curried<#ty, #ctx_ty>), body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MapFs;

    /// A file system with nothing in it: the inline form never looks.
    fn no_files() -> MapFs {
        MapFs::touch(&[])
    }

    /// The declaring file a test names — or, for `None`, a call site the
    /// compiler said nothing about at all.
    fn source(file: Option<&str>) -> Source<'_> {
        match file.map(Path::new) {
            Some(file) => Source::Found(file),
            None => Source::Unknown(&[]),
        }
    }

    /// Expand `item` (with `attr` as `#[group]`'s own arguments) to a token
    /// string.
    fn expansion(attr: &str, item: &str) -> String {
        let args: GroupArgs = syn::parse_str(attr).expect("the attribute parses");
        let item: Item = syn::parse_str(item).expect("the item parses");
        expand(&no_files(), source(None), args, item)
            .expect("is a group")
            .to_string()
    }

    /// The error `#[group]` raises for `item`.
    fn rejection(attr: &str, item: &str) -> String {
        let args: GroupArgs = syn::parse_str(attr).expect("the attribute parses");
        let item: Item = syn::parse_str(item).expect("the item parses");
        expand(&no_files(), source(None), args, item)
            .expect_err("is not a group")
            .to_string()
    }

    /// Expand `#[group(attr)]` on the file module `item`, declared in `file`,
    /// against `tree`.
    fn file_expansion(tree: &[(&str, &str)], file: &str, attr: &str, item: &str) -> String {
        let args: GroupArgs = syn::parse_str(attr).expect("the attribute parses");
        let item: Item = syn::parse_str(item).expect("the item parses");
        expand(&MapFs::new(tree), source(Some(file)), args, item)
            .expect("expands")
            .to_string()
    }

    /// Every error `#[group(attr)]` raises on the file module `item`.
    fn file_rejections(
        tree: &[(&str, &str)],
        file: Option<&str>,
        attr: &str,
        item: &str,
    ) -> Vec<String> {
        let args: GroupArgs = syn::parse_str(attr).expect("the attribute parses");
        let item: Item = syn::parse_str(item).expect("the item parses");
        expand(&MapFs::new(tree), source(file), args, item)
            .expect_err("does not expand")
            .into_iter()
            .map(|e| e.to_string())
            .collect()
    }

    /// The one error `#[group]` raises on the file module `item`.
    fn file_rejection(tree: &[(&str, &str)], file: Option<&str>, item: &str) -> String {
        let mut all = file_rejections(tree, file, "", item);
        assert_eq!(all.len(), 1, "expected exactly one error: {all:?}");
        all.remove(0)
    }

    #[test]
    fn a_two_child_group_is_re_emitted_with_a_handle_beside_it() {
        // The reference expansion, asserted whole: everything the macro does is
        // visible in one place, and a change to any of it has to be written
        // down here first. Note `create`'s `#[cfg]` on its *push*, and the
        // module itself coming back with its inner attributes untouched — rustc
        // expands those next.
        let got = expansion(
            "",
            "/// events and instances\n\
             ///\n\
             /// An event is a directory; an instance is a branch of it.\n\
             pub mod event {\n\
                 /// create an event\n\
                 #[cfg(unix)]\n\
                 #[command]\n\
                 pub fn create(dir: EventDir, ctx: &Ctx) -> Outcome { work() }\n\
                 /// manage audiences\n\
                 #[group]\n\
                 pub mod audience { /// define\n#[command] pub fn define(c: &Ctx) {} }\n\
             }",
        );
        let want = quote! {
            #[doc = " events and instances"]
            #[doc = ""]
            #[doc = " An event is a directory; an instance is a branch of it."]
            pub mod event {
                #[doc = " create an event"]
                #[cfg(unix)]
                #[command]
                pub fn create(dir: EventDir, ctx: &Ctx) -> Outcome { work() }
                #[doc = " manage audiences"]
                #[group]
                pub mod audience {
                    #[doc = " define"]
                    #[command]
                    pub fn define(c: &Ctx) {}
                }
            }

            #[doc = "`hicli` registration handle for [`event`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_event() -> ::hicli::Command<Ctx> {
                #[allow(unused_mut)]
                let mut __children: ::std::vec::Vec<::hicli::Command<Ctx>> =
                    ::std::vec::Vec::new();
                #[cfg(unix)]
                __children.push(event::__hicli_create());
                __children.push(event::__hicli_audience());
                ::hicli::group("event", "events and instances", __children)
                .details("An event is a directory; an instance is a branch of it.")
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_context_free_group_is_generic_over_ctx() {
        let got = expansion(
            "",
            "/// tools\npub mod tools { /// v\n#[command] fn version() {} }",
        );
        assert!(
            got.contains(
                "pub fn __hicli_tools < Ctx : 'static > () -> :: hicli :: Command < Ctx >"
            ),
            "{got}"
        );
    }

    #[test]
    fn the_ident_becomes_the_name_with_dashes() {
        let got = expansion(
            "",
            "/// manage administrators\nmod add_admin { /// g\n#[command] fn grant(c: &Ctx) {} }",
        );
        assert!(got.contains("\"add-admin\""), "{got}");
        assert!(got.contains("pub fn __hicli_add_admin"), "{got}");
    }

    #[test]
    fn a_one_line_doc_comment_has_no_details() {
        let got = expansion(
            "",
            "/// identity\nmod id { /// c\n#[command] fn create(c: &Ctx) {} }",
        );
        assert!(!got.contains(". details"), "{got}");
    }

    #[test]
    fn a_cfg_is_copied_onto_the_handle() {
        let got = expansion(
            "",
            "/// only here\n#[cfg(unix)]\nmod only { /// g\n#[command] fn go(c: &Ctx) {} }",
        );
        assert!(got.contains("# [cfg (unix)] pub fn __hicli_only"), "{got}");
    }

    #[test]
    fn a_module_without_a_doc_comment_is_rejected() {
        assert_eq!(
            rejection("", "mod event { /// c\n#[command] fn create(c: &Ctx) {} }"),
            GROUP_NEEDS_DOC
        );
    }

    #[test]
    fn a_blank_doc_comment_is_rejected_too() {
        assert_eq!(
            rejection(
                "",
                "///\n///\nmod event { /// c\n#[command] fn create(c: &Ctx) {} }"
            ),
            GROUP_NEEDS_DOC
        );
    }

    #[test]
    fn an_empty_group_is_rejected() {
        assert_eq!(
            rejection(
                "",
                "/// a group nobody can enter\nmod empty { use super::*; }"
            ),
            "group `empty` has no commands: add a #[command] function or a nested #[group] module \
             to its body"
        );
    }

    #[test]
    fn a_non_module_item_is_rejected() {
        assert_eq!(rejection("", "/// help\nfn status() {}"), NOT_A_MODULE);
    }

    #[test]
    fn a_file_group_inside_an_inline_group_is_rejected() {
        // The child's own `#[group]` would resolve `audience` against the
        // declaring file; rustc resolves it against `event/`. Every such child
        // is named, at its own ident.
        let all: Vec<String> = expand(
            &no_files(),
            source(None),
            GroupArgs::default(),
            syn::parse_str(
                "/// events\n\
                 mod event {\n\
                     /// c\n#[command] fn create(c: &Ctx) {}\n\
                     /// a\n#[group] pub mod audience;\n\
                     /// r\n#[group] pub mod roster;\n\
                 }",
            )
            .expect("the item parses"),
        )
        .expect_err("is not a group")
        .into_iter()
        .map(|e| e.to_string())
        .collect();
        assert_eq!(
            all,
            [
                "`#[group] mod audience;` inside an inline module resolves against the wrong \
                 directory; make `audience` inline, or move the declaring module to its own file",
                "`#[group] mod roster;` inside an inline module resolves against the wrong \
                 directory; make `roster` inline, or move the declaring module to its own file",
            ]
        );
    }

    #[test]
    fn a_file_group_with_its_own_path_inside_an_inline_group_is_allowed() {
        // An explicit `#[path]` means the same file to rustc and to the child's
        // own `#[group]`, so there is nothing to disagree about.
        let got = expansion(
            "",
            "/// events\n\
             mod event {\n\
                 /// a\n#[group] #[path = \"audience.rs\"] pub mod audience;\n\
             }",
        );
        assert!(got.contains("event :: __hicli_audience ()"), "{got}");
    }

    // --- the file form --------------------------------------------------------

    /// A two-level tree: a cfg'd command, a nested inline `#[group]`, and a nested
    /// file `#[group]` — the three shapes a child comes in, one of each, read
    /// out of a file.
    fn event_tree() -> Vec<(&'static str, &'static str)> {
        vec![
            ("src/main.rs", ""),
            (
                "src/event.rs",
                "/// create an event\n\
                 #[cfg(unix)]\n\
                 #[command]\n\
                 pub fn create(dir: EventDir, ctx: &Ctx) -> Outcome { work() }\n\
                 /// manage instances\n\
                 #[group]\n\
                 pub mod instance { /// branch\n#[command] pub fn branch(c: &Ctx) {} }\n\
                 /// manage audiences\n\
                 #[group]\n\
                 pub mod audience;\n",
            ),
            (
                "src/event/audience.rs",
                "/// define an audience\n#[command] pub fn define(c: &Ctx) {}\n",
            ),
        ]
    }

    #[test]
    fn a_file_group_beside_a_crate_root_is_re_emitted_without_a_path() {
        // The reference expansion for the file form, asserted whole. `main.rs`
        // is mod-rs by name and `src/event.rs` is beside it, so rustc finds the
        // file unaided and the declaration goes out as written — no `#[path]`,
        // which would make `event.rs` mod-rs too (#25). The handle is written
        // from children the macro found by *reading* `src/event.rs`, in the
        // file's source order.
        let got = file_expansion(
            &event_tree(),
            "src/main.rs",
            "",
            "/// events and instances\n\
             ///\n\
             /// An event is a directory; an instance is a branch of it.\n\
             pub mod event;",
        );
        let want = quote! {
            #[doc = " events and instances"]
            #[doc = ""]
            #[doc = " An event is a directory; an instance is a branch of it."]
            pub mod event;

            #[doc = "`hicli` registration handle for [`event`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_event() -> ::hicli::Command<Ctx> {
                #[allow(unused_mut)]
                let mut __children: ::std::vec::Vec<::hicli::Command<Ctx>> =
                    ::std::vec::Vec::new();
                #[cfg(unix)]
                __children.push(event::__hicli_create());
                __children.push(event::__hicli_instance());
                __children.push(event::__hicli_audience());
                ::hicli::group("event", "events and instances", __children)
                .details("An event is a directory; an instance is a branch of it.")
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_ctx_given_to_group_on_a_file_module_is_the_named_ctx() {
        // The explicit context wins over the module-relative one the file's
        // command names — which could not be lifted — and the declaration goes
        // back out as a plain `mod`.
        let got = file_expansion(
            &[
                ("src/main.rs", ""),
                (
                    "src/id.rs",
                    "/// c\n#[command] pub fn create(c: &super::Ctx) {}",
                ),
            ],
            "src/main.rs",
            "ctx = Ctx",
            "/// identity\npub mod id;",
        );
        assert!(
            got.contains("pub fn __hicli_id () -> :: hicli :: Command < Ctx >"),
            "{got}"
        );
        assert!(!got.contains("# [group"), "a #[group] was emitted:\n{got}");
    }

    #[test]
    fn a_file_group_under_a_root_with_its_own_directory_keeps_a_path() {
        // `tests/cli.rs` is a crate root, so rustc would look in `tests/` —
        // the `#[path]` is what makes `tests/cli/event.rs` the answer.
        let got = file_expansion(
            &[
                ("tests/cli.rs", ""),
                (
                    "tests/cli/event.rs",
                    "/// c\n#[command] pub fn create(c: &Ctx) {}",
                ),
            ],
            "tests/cli.rs",
            "",
            "/// events\npub mod event;",
        );
        assert!(got.contains("# [path = \"cli/event.rs\"]"), "{got}");
    }

    #[test]
    fn a_nested_file_group_in_an_ordinary_file_has_no_path() {
        // `src/event.rs` is loaded by `main.rs`'s own plain declaration, so it
        // is not mod-rs, and rustc looks in `src/event/` unaided.
        let got = file_expansion(
            &[
                ("src/main.rs", "/// e\n#[group]\npub mod event;"),
                ("src/event.rs", ""),
                (
                    "src/event/audience.rs",
                    "/// d\n#[command] pub fn define(c: &Ctx) {}",
                ),
            ],
            "src/event.rs",
            "",
            "/// a\npub mod audience;",
        );
        assert!(!got.contains("# [path"), "a #[path] was emitted:\n{got}");
    }

    #[test]
    fn an_authors_own_path_is_left_alone() {
        let got = file_expansion(
            &[
                ("src/main.rs", ""),
                (
                    "src/other/dir.rs",
                    "/// c\n#[command] pub fn create(c: &Ctx) {}",
                ),
            ],
            "src/main.rs",
            "",
            "/// events\n#[path = \"other/dir.rs\"]\npub mod event;",
        );
        assert_eq!(
            got.matches("# [path").count(),
            1,
            "a second #[path] was synthesised:\n{got}"
        );
        assert!(got.contains("# [path = \"other/dir.rs\"]"), "{got}");
    }

    #[test]
    fn a_context_free_file_group_is_generic_over_ctx() {
        let got = file_expansion(
            &[
                ("src/main.rs", ""),
                ("src/tools.rs", "/// v\n#[command] pub fn version() {}"),
            ],
            "src/main.rs",
            "",
            "/// the odds and ends\npub mod tools;",
        );
        assert!(
            got.contains(
                "pub fn __hicli_tools < Ctx : 'static > () -> :: hicli :: Command < Ctx >"
            ),
            "{got}"
        );
    }

    #[test]
    fn a_context_named_only_in_a_grandchild_file_is_found() {
        // Nothing in `event.rs` names a context; `event/audience.rs` does, and
        // the walk crosses the file boundary to reach it.
        let got = file_expansion(
            &[
                ("src/main.rs", ""),
                (
                    "src/event.rs",
                    "/// manage audiences\n#[group]\npub mod audience;",
                ),
                (
                    "src/event/audience.rs",
                    "/// d\n#[command] pub fn define(c: &crate::Ctx) {}",
                ),
            ],
            "src/main.rs",
            "",
            "/// events\npub mod event;",
        );
        assert!(
            got.contains("-> :: hicli :: Command < crate :: Ctx >"),
            "{got}"
        );
    }

    #[test]
    fn a_declaration_whose_file_is_missing_lists_the_candidates() {
        assert_eq!(
            file_rejection(
                &[("src/main.rs", "")],
                Some("src/main.rs"),
                "/// events\npub mod event;"
            ),
            "cannot find the file for module `event`: tried `src/event.rs`, `src/event/mod.rs`\n\
             note: #[group] on a file module must be at the top level of its file; inside an \
             inline `mod`, use an inline #[group] mod instead"
        );
    }

    #[test]
    fn a_file_that_is_not_rust_says_which_file() {
        let e = file_rejection(
            &[("src/main.rs", ""), ("src/event.rs", "pub fn (")],
            Some("src/main.rs"),
            "/// events\npub mod event;",
        );
        assert!(
            e.starts_with("cannot read module `event` from `src/event.rs`: "),
            "{e}"
        );
    }

    #[test]
    fn a_file_with_no_commands_names_the_file() {
        assert_eq!(
            file_rejection(
                &[("src/main.rs", ""), ("src/event.rs", "pub fn helper() {}")],
                Some("src/main.rs"),
                "/// events\npub mod event;"
            ),
            "group `event` has no commands: add a #[command] function or a nested #[group] module \
             to `src/event.rs`"
        );
    }

    #[test]
    fn a_declaration_without_a_doc_comment_is_rejected_before_the_file_is_read() {
        // Help is mandatory whichever form declared the group, and it is the
        // first thing checked: there is no point resolving a file for a module
        // that cannot be a group anyway.
        assert_eq!(
            file_rejection(&[], Some("src/main.rs"), "pub mod event;"),
            GROUP_NEEDS_DOC
        );
    }

    #[test]
    fn an_unlocatable_source_file_names_the_inline_form() {
        // D6: without a file to resolve against there is nothing to do but say
        // so, and name the form that needs no file.
        assert_eq!(
            file_rejection(&[], None, "/// events\npub mod event;"),
            "hicli cannot locate the source file of this #[group] file module (the path is \
             remapped or synthetic); use an inline #[group] mod event { … }"
        );
    }

    #[test]
    fn an_unlocatable_source_file_names_what_was_looked_for() {
        // The path the compiler reported, said out loud: under a front end that
        // is not cargo it is the only clue to why nothing opened.
        let tried = ["examples/files/main.rs".to_string()];
        let item: Item = syn::parse_str("/// events\npub mod event;").expect("the item parses");
        let e = expand(
            &no_files(),
            Source::Unknown(&tried),
            GroupArgs::default(),
            item,
        )
        .expect_err("does not expand")
        .to_string();
        assert_eq!(
            e,
            "hicli cannot locate the source file of this #[group] file module (tried \
             `examples/files/main.rs` under the working directory and the manifest directory and \
             its parents); use an inline #[group] mod event { … }"
        );
    }

    #[test]
    fn an_alias_equal_to_a_file_groups_name_is_rejected() {
        // Checked before the file is looked for: the tree is empty.
        assert_eq!(
            file_rejections(
                &[],
                Some("src/main.rs"),
                "alias = \"add-admin\"",
                "/// admins\npub mod add_admin;"
            ),
            ["alias `add-admin` is this group's own name: an alias is another name"]
        );
    }

    /// The error parsing `attr` as `#[group]`'s arguments raises.
    fn arg_rejection(attr: &str) -> String {
        match syn::parse_str::<GroupArgs>(attr) {
            Ok(_) => panic!("`{attr}` was accepted as a #[group] argument"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn an_unknown_argument_is_rejected() {
        // The one that will be tried: `#[command]`'s argument, on a group.
        assert_eq!(arg_rejection("name = \"events\""), UNKNOWN_ARG);
        // … and something that is not `key = value` at all.
        assert_eq!(arg_rejection("\"events\""), UNKNOWN_ARG);
        assert_eq!(arg_rejection("ctx = Ctx, name = \"x\""), UNKNOWN_ARG);
    }

    #[test]
    fn each_shared_key_rejection_is_raised_on_a_group_too() {
        assert_eq!(arg_rejection("ctx = A, ctx = B"), "`ctx` is given twice");
        assert_eq!(arg_rejection("hide, hide"), "`hide` is given twice");
        assert_eq!(arg_rejection("alias = \"\""), "an alias cannot be empty");
        assert_eq!(
            arg_rejection("alias = \"e v\""),
            "an alias is one word: it cannot contain whitespace"
        );
        assert_eq!(
            arg_rejection("alias = \"-e\""),
            "an alias is a command name, not a flag: it cannot start with `-`"
        );
        assert_eq!(
            arg_rejection("alias = \"e\", alias = \"e\""),
            "alias `e` is given twice"
        );
    }

    #[test]
    fn an_alias_equal_to_the_groups_name_is_rejected() {
        assert_eq!(
            rejection(
                "alias = \"add-admin\"",
                "/// admins\nmod add_admin { /// g\n#[command] fn grant(c: &Ctx) {} }"
            ),
            "alias `add-admin` is this group's own name: an alias is another name"
        );
    }

    #[test]
    fn the_keys_and_the_examples_become_calls_on_the_group() {
        let got = expansion(
            "ctx = Ctx, alias = \"e\", hide, alias = \"ev\"",
            "/// events\n\
             ///\n\
             /// All of them.\n\
             ///\n\
             /// # Examples\n\
             ///\n\
             /// ```sh\n\
             /// demo event create x\n\
             /// ```\n\
             mod event { /// c\n#[command] fn create(c: &Ctx) {} }",
        );
        assert!(
            got.ends_with(
                ":: hicli :: group (\"event\" , \"events\" , __children) . details (\"All of them.\") \
                 . footer (\"Examples:\\n  demo event create x\") . alias (\"e\") . alias (\"ev\") \
                 . hide () }"
            ),
            "{got}"
        );
    }

    // --- curried arguments (ADR-011) -----------------------------------------

    #[test]
    fn an_args_group_sorts_its_children_into_fixed_and_bound() {
        // The reference expansion for `args = T`, asserted whole. `list` takes
        // no source name and is fixed; `sync` (cfg'd) and the nested curried
        // group `jobs` receive one and are bound.
        let got = expansion(
            "args = SourceName",
            "/// music sources\n\
             pub mod source {\n\
                 /// list sources\n\
                 #[command]\n\
                 pub fn list(ctx: &Ctx) {}\n\
                 /// sync a source\n\
                 #[cfg(unix)]\n\
                 #[command(curried)]\n\
                 pub fn sync(s: SourceName, ctx: &Ctx) {}\n\
                 /// a source's jobs\n\
                 #[group(curried = SourceName)]\n\
                 pub mod jobs { /// c\n#[command(curried)] pub fn cancel(s: SourceName) {} }\n\
             }",
        );
        let want = quote! {
            #[doc = " music sources"]
            pub mod source {
                #[doc = " list sources"]
                #[command]
                pub fn list(ctx: &Ctx) {}
                #[doc = " sync a source"]
                #[cfg(unix)]
                #[command(curried)]
                pub fn sync(s: SourceName, ctx: &Ctx) {}
                #[doc = " a source's jobs"]
                #[group(curried = SourceName)]
                pub mod jobs {
                    #[doc = " c"]
                    #[command(curried)]
                    pub fn cancel(s: SourceName) {}
                }
            }

            #[doc = "`hicli` registration handle for [`source`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_source() -> ::hicli::Command<Ctx> {
                #[allow(unused_mut)]
                let mut __fixed: ::std::vec::Vec<::hicli::Command<Ctx>> =
                    ::std::vec::Vec::new();
                #[allow(unused_mut)]
                let mut __bound: ::std::vec::Vec<::hicli::Curried<SourceName, Ctx>> =
                    ::std::vec::Vec::new();
                __fixed.push(source::__hicli_list());
                #[cfg(unix)]
                __bound.push(::hicli::__CurriedOver::<SourceName, Ctx>::__node(
                    source::__hicli_sync()
                ));
                __bound.push(::hicli::__CurriedOver::<SourceName, Ctx>::__node(
                    source::__hicli_jobs()
                ));
                ::hicli::group_args(
                    "source",
                    "music sources",
                    <SourceName as ::hicli::Arg>::parser,
                    __fixed,
                    __bound
                )
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_curried_group_is_a_curried_node_itself() {
        // The reference expansion for `curried = T`: one vector of curried
        // children, `hicli::group`, and a `Curried<T, Ctx>` return.
        let got = expansion(
            "curried = SourceName, alias = \"j\"",
            "/// a source's jobs\n\
             pub mod jobs {\n\
                 /// cancel a job\n\
                 #[command(curried)]\n\
                 pub fn cancel(s: SourceName, id: JobId, ctx: &Ctx) {}\n\
             }",
        );
        let want = quote! {
            #[doc = " a source's jobs"]
            pub mod jobs {
                #[doc = " cancel a job"]
                #[command(curried)]
                pub fn cancel(s: SourceName, id: JobId, ctx: &Ctx) {}
            }

            #[doc = "`hicli` registration handle for [`jobs`]."]
            #[doc(hidden)]
            #[allow(unreachable_pub)]
            pub fn __hicli_jobs() -> ::hicli::Curried<SourceName, Ctx> {
                #[allow(unused_mut)]
                let mut __children: ::std::vec::Vec<::hicli::Curried<SourceName, Ctx>> =
                    ::std::vec::Vec::new();
                __children.push(::hicli::__CurriedOver::<SourceName, Ctx>::__node(
                    jobs::__hicli_cancel()
                ));
                ::hicli::group("jobs", "a source's jobs", __children)
                .alias("j")
            }
        }
        .to_string();
        assert_eq!(got, want);
    }

    #[test]
    fn a_context_free_args_group_is_generic_over_ctx() {
        let got = expansion(
            "args = SourceName",
            "/// sources\nmod source { /// s\n#[command(curried)] fn show(s: SourceName) {} }",
        );
        assert!(
            got.contains(
                "pub fn __hicli_source < Ctx : 'static > () -> :: hicli :: Command < Ctx >"
            ),
            "{got}"
        );
        assert!(
            got.contains(":: hicli :: Curried < SourceName , Ctx >"),
            "{got}"
        );
    }

    #[test]
    fn a_context_free_curried_group_is_generic_over_ctx() {
        let got = expansion(
            "curried = SourceName",
            "/// jobs\nmod jobs { /// s\n#[command(curried)] fn show(s: SourceName) {} }",
        );
        assert!(
            got.contains(
                "pub fn __hicli_jobs < Ctx : 'static > () -> :: hicli :: Curried < SourceName , \
                 Ctx >"
            ),
            "{got}"
        );
    }

    #[test]
    fn an_args_file_group_reads_its_children_out_of_the_file() {
        let got = file_expansion(
            &[
                ("src/main.rs", ""),
                (
                    "src/source.rs",
                    "/// list sources\n#[command] pub fn list(ctx: &Ctx) {}\n\
                     /// sync a source\n#[command(curried)] pub fn sync(s: SourceName, ctx: &Ctx) {}\n\
                     /// jobs\n#[group(curried = SourceName)] pub mod jobs;\n",
                ),
            ],
            "src/main.rs",
            "args = SourceName",
            "/// music sources\npub mod source;",
        );
        assert!(
            got.contains(
                "__fixed . push (source :: __hicli_list ()) ; \
                 __bound . push (:: hicli :: __CurriedOver :: < SourceName , Ctx > :: __node \
                 (source :: __hicli_sync ())) ; \
                 __bound . push (:: hicli :: __CurriedOver :: < SourceName , Ctx > :: __node \
                 (source :: __hicli_jobs ())) ; \
                 :: hicli :: group_args (\"source\" , \"music sources\" , < SourceName as :: \
                 hicli :: Arg > :: parser , __fixed , __bound) }"
            ),
            "{got}"
        );
        assert!(
            got.contains("pub fn __hicli_source () -> :: hicli :: Command < Ctx >"),
            "{got}"
        );
    }

    #[test]
    fn a_default_command_in_a_plain_group_is_pushed_like_any_other() {
        let got = expansion(
            "",
            "/// search\nmod search {\n\
                 /// search the library\n#[command(default)] fn library(q: Query, c: &Ctx) {}\n\
                 /// list fact values\n#[command] fn fact_values_for(t: FactType, c: &Ctx) {}\n\
             }",
        );
        assert!(
            got.contains(
                "__children . push (search :: __hicli_library ()) ; \
                 __children . push (search :: __hicli_fact_values_for ()) ;"
            ),
            "{got}"
        );
    }

    /// Every error `#[group(attr)]` raises on the inline module `item`.
    fn rejections(attr: &str, item: &str) -> Vec<String> {
        let args: GroupArgs = syn::parse_str(attr).expect("the attribute parses");
        let item: Item = syn::parse_str(item).expect("the item parses");
        expand(&no_files(), source(None), args, item)
            .expect_err("is not a group")
            .into_iter()
            .map(|e| e.to_string())
            .collect()
    }

    #[test]
    fn a_curried_child_of_a_plain_group_is_rejected() {
        assert_eq!(
            rejections(
                "",
                "/// sources\nmod source {\n\
                     /// s\n#[command(curried)] fn sync(s: SourceName) {}\n\
                     /// j\n#[group(curried = SourceName)] mod jobs { /// c\n#[command(curried)] fn c(s: SourceName) {} }\n\
                     /// l\n#[command] fn list() {}\n\
                 }"
            ),
            [
                "`sync` is curried, but group `source` has no argument to hand it: declare one \
                 with #[group(args = <Type>)]",
                "`jobs` is curried, but group `source` has no argument to hand it: declare one \
                 with #[group(args = <Type>)]",
            ]
        );
    }

    #[test]
    fn an_args_group_with_no_curried_child_is_rejected() {
        assert_eq!(
            rejections(
                "args = SourceName",
                "/// sources\nmod source { /// l\n#[command] fn list() {} }"
            ),
            [
                "group `source` takes `args = SourceName`, but none of its children receives it: \
                 mark the commands that take it #[command(curried)]"
            ]
        );
    }

    #[test]
    fn an_unmarked_child_of_a_curried_group_is_rejected() {
        assert_eq!(
            rejections(
                "curried = crate::SourceName",
                "/// jobs\nmod jobs {\n\
                     /// c\n#[command(curried)] fn cancel(s: crate::SourceName) {}\n\
                     /// l\n#[command] fn list() {}\n\
                     /// g\n#[group] mod more { /// m\n#[command] fn m() {} }\n\
                     /// a\n#[group(args = Other)] mod other { /// o\n#[command(curried)] fn o(x: Other) {} }\n\
                 }"
            ),
            [
                "`list` is in group `jobs`, which hands every child a `crate::SourceName`: mark it \
                 #[command(curried)] and take the `crate::SourceName` as its first parameter",
                "`more` is in group `jobs`, which hands every child a `crate::SourceName`: mark it \
                 #[group(curried = crate::SourceName)]",
                "`other` takes `args` of its own inside group `jobs`, which hands every child a \
                 `crate::SourceName`: a group with `args` inside a curried group is not supported \
                 yet",
            ]
        );
    }

    #[test]
    fn two_default_children_are_rejected_at_the_second() {
        assert_eq!(
            rejections(
                "",
                "/// search\nmod search {\n\
                     /// a\n#[command(default)] fn a() {}\n\
                     /// b\n#[command(default)] fn b() {}\n\
                     /// c\n#[command(default)] fn c() {}\n\
                 }"
            ),
            [
                "group `search` already has a default command, `a`: a group has at most one",
                "group `search` already has a default command, `a`: a group has at most one",
            ]
        );
    }

    #[test]
    fn a_default_child_of_a_group_with_an_argument_is_rejected() {
        assert_eq!(
            rejections(
                "args = SourceName",
                "/// sources\nmod source {\n\
                     /// l\n#[command(default)] fn list() {}\n\
                     /// s\n#[command(curried)] fn sync(s: SourceName) {}\n\
                 }"
            ),
            [
                "`list` cannot be a default command: group `source` takes an argument, so it has \
                 no default command"
            ]
        );
        // In a curried group the default child is also unmarked: both are
        // said.
        assert_eq!(
            rejections(
                "curried = SourceName",
                "/// jobs\nmod jobs {\n\
                     /// l\n#[command(default)] fn list() {}\n\
                     /// s\n#[command(curried)] fn sync(s: SourceName) {}\n\
                 }"
            ),
            [
                "`list` is in group `jobs`, which hands every child a `SourceName`: mark it \
                 #[command(curried)] and take the `SourceName` as its first parameter",
                "`list` cannot be a default command: group `jobs` receives an argument, so it has \
                 no default command",
            ]
        );
    }

    #[test]
    fn a_file_group_checks_its_children_too() {
        assert_eq!(
            file_rejections(
                &[
                    ("src/main.rs", ""),
                    (
                        "src/source.rs",
                        "/// s\n#[command(curried)] pub fn sync(s: SourceName) {}"
                    ),
                ],
                Some("src/main.rs"),
                "",
                "/// sources\npub mod source;"
            ),
            [
                "`sync` is curried, but group `source` has no argument to hand it: declare one \
                 with #[group(args = <Type>)]"
            ]
        );
    }

    #[test]
    fn each_curried_key_rejection_is_raised() {
        assert_eq!(arg_rejection("args = A, args = B"), "`args` is given twice");
        assert_eq!(
            arg_rejection("curried = A, curried = B"),
            "`curried` is given twice"
        );
        assert_eq!(
            arg_rejection("args"),
            "`args` on a group names a type: write `args = <Type>`"
        );
        assert_eq!(
            arg_rejection("curried"),
            "`curried` on a group names a type: write `curried = <Type>`"
        );
        assert_eq!(arg_rejection("args = A, curried = A"), ARGS_AND_CURRIED);
        assert_eq!(arg_rejection("curried = A, args = A"), ARGS_AND_CURRIED);
        // A group is never a default command — and is told where `default`
        // goes instead.
        assert_eq!(arg_rejection("default"), GROUP_DEFAULT);
        assert_eq!(arg_rejection("ctx = Ctx, default"), GROUP_DEFAULT);
    }

    #[test]
    fn keys_given_to_group_on_a_file_module_become_calls() {
        let got = file_expansion(
            &[
                ("src/main.rs", ""),
                ("src/id.rs", "/// c\n#[command] pub fn create(c: &Ctx) {}"),
            ],
            "src/main.rs",
            "alias = \"identity\", hide",
            "/// identity\npub mod id;",
        );
        assert!(got.contains(". alias (\"identity\") . hide ()"), "{got}");
        assert!(!got.contains("# [group"), "a #[group] was emitted:\n{got}");
    }
}
