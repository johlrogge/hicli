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

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, Ident, Item, ItemMod, LitStr, Result, Token, Type};

use crate::ctx::{self, Ctx, Outline};
use crate::discover::{children_of, Child, ChildKind};
use crate::expand::metadata;
use crate::fs::Fs;
use crate::resolve::{resolve as resolve_file, show};
use crate::shared::{
    cfg_attrs, dashed, handle_doc, handle_ident, parse_keys, path_attr, read_doc, unraw_name,
    CommandMeta, GROUP_NEEDS_DOC,
};

/// The message for an argument `#[group]` does not take (D10).
const UNKNOWN_ARG: &str =
    "#[group] takes `ctx = <Type>`, `alias = \"...\"` (repeatable) and `hide`";

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

/// What `#[group]` itself was given: `ctx = <Type>`, `alias = "…"`
/// (repeatable) and `hide`, each optional.
#[derive(Default)]
pub(crate) struct GroupArgs {
    /// The context type, said out loud instead of lifted from a child.
    ctx: Option<Type>,
    /// The aliases and the visibility, read by the rules `#[command]` shares.
    meta: CommandMeta,
}

impl Parse for GroupArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut ctx: Option<Type> = None;
        let meta = parse_keys(input, UNKNOWN_ARG, |key, input| {
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
        Ok(GroupArgs { ctx, meta })
    }
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
    let handle = emit_handle(&Handle {
        module: &item.ident,
        name,
        help: doc.help,
        details: doc.details,
        footer: doc.footer,
        aliases: args.meta.aliases,
        hide: args.meta.hide.is_some(),
        cfgs: cfg_attrs(&item.attrs),
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
        if let ChildKind::Outlined { path: None } = child.kind {
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
/// The declaration comes back with a synthesised `#[path]` — so rustc reads
/// exactly the bytes that were read here, and the one question a proc macro
/// cannot answer (is this declaring file a crate root?) stops mattering — and
/// the handle comes back beside it, written from the children found in the
/// file.
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
        ctx,
        children,
    });

    // Never an absolute path: `relative` is relative to the declaring file's
    // directory, which is what `#[path]` on an outlined module means (spike
    // 0.4). An author who wrote their own keeps it.
    if explicit.is_none() {
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

    let module = h.module;
    // Each child's handle is called *directly*, not through `IntoCommand`, so a
    // child whose context is a different type is a plain type error at its own
    // `push` rather than an unsatisfied trait bound on the whole group.
    let pushes = h.children.iter().map(|c| {
        let cfgs = &c.cfgs;
        let child = handle_ident(&c.ident);
        quote! {
            #(#cfgs)*
            __children.push(#module::#child());
        }
    });

    let handle = handle_ident(h.module);
    let doc = handle_doc(h.module);
    let cfgs = &h.cfgs;
    let name = &h.name;
    let help = &h.help;
    let metadata = metadata(
        h.details.as_deref(),
        h.footer.as_deref(),
        &h.aliases,
        h.hide,
    );

    quote! {
        #[doc = #doc]
        #[doc(hidden)]
        #[allow(unreachable_pub)]
        #(#cfgs)*
        pub fn #handle #generics () -> ::hicli::Command<#ctx_ty> {
            // Every child may be configured out, and then nothing is pushed.
            #[allow(unused_mut)]
            let mut __children: ::std::vec::Vec<::hicli::Command<#ctx_ty>> =
                ::std::vec::Vec::new();
            #(#pushes)*
            ::hicli::group(#name, #help, __children)
            #metadata
        }
    }
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
    fn a_file_group_is_re_emitted_with_a_path_and_a_handle() {
        // The reference expansion for the file form, asserted whole. Note the
        // synthesised `#[path]` — relative, never absolute — and that the
        // handle is written from children the macro found by *reading*
        // `src/event.rs`, in the file's source order.
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
            #[path = "event.rs"]
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
