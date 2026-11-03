//! Reading a `#[command]` function: the attribute's arguments, the fn's name,
//! its doc comment and its parameters.
//!
//! Everything the macro needs is decided here, so [`expand`](crate::expand)
//! never has to make a judgement call — and every rejection is raised with a
//! span pointing at the token the author has to change.

use quote::format_ident;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, FnArg, Ident, ItemFn, LitStr, Pat, Result, Signature, Token, Type};

use crate::shared::{
    cfg_attrs, check_spelling, dashed, handle_ident, parse_keys, read_doc, CommandMeta, Spelling,
    COMMAND_NEEDS_DOC,
};

/// The message for an argument `#[command]` does not take.
const UNKNOWN_ARG: &str =
    "#[command] takes `name = \"...\"`, `alias = \"...\"` (repeatable) and `hide`";

/// What `#[command]` itself was given: `name = "..."`, `alias = "..."`
/// (repeatable) and `hide`, each optional.
#[derive(Default)]
pub(crate) struct CommandArgs {
    /// An explicit command name, overriding the ident.
    name: Option<LitStr>,
    /// The aliases and the visibility, read by the rules `#[group]` shares.
    meta: CommandMeta,
}

impl Parse for CommandArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut name: Option<LitStr> = None;
        let meta = parse_keys(input, UNKNOWN_ARG, |key, input| {
            if key != "name" {
                return Ok(false);
            }
            if name.is_some() {
                return Err(Error::new_spanned(key, "`name` is given twice"));
            }
            let _: Token![=] = input.parse()?;
            let lit: LitStr = input.parse()?;
            check_spelling(&lit, Spelling::Name)?;
            name = Some(lit);
            Ok(true)
        })?;
        Ok(CommandArgs { name, meta })
    }
}

/// One parameter of the original function, in declaration order — which is the
/// order the body is called in.
pub(crate) enum Slot {
    /// An owned argument, bound by the generated closure under this name.
    Arg(Ident),
    /// The shared context.
    Ctx,
}

/// Everything `#[command]` learned from the function it was written on.
pub(crate) struct CommandFn {
    /// The command's name on the command line.
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
    /// The function's own ident — what the handle's closure calls.
    pub(crate) ident: Ident,
    /// The handle's ident: `__hicli_` + the fn's ident, unescaped.
    pub(crate) handle: Ident,
    /// The `#[cfg]` / `#[cfg_attr]` attributes, *copied* onto the handle so it
    /// is configured out together with the function it registers.
    pub(crate) cfg_attrs: Vec<Attribute>,
    /// The type of each owned parameter, **as written**, in declaration order.
    /// Each is resolved through `Arg` at expansion time, spanned where the
    /// author wrote it.
    pub(crate) arg_tys: Vec<Type>,
    /// The closure binding for each owned parameter, in declaration order.
    pub(crate) bindings: Vec<Ident>,
    /// The original parameter order, for the call into the function.
    pub(crate) slots: Vec<Slot>,
    /// The context type `T` of the single `&T` parameter, if there is one.
    pub(crate) ctx_ty: Option<Type>,
    /// The function itself, re-emitted verbatim — name, signature, visibility,
    /// attributes (in their original order) and body, all untouched.
    pub(crate) item: ItemFn,
}

impl CommandFn {
    /// Read `item`, or explain why it cannot be a command.
    pub(crate) fn from_item(args: CommandArgs, item: ItemFn) -> Result<CommandFn> {
        check_signature(&item.sig)?;
        let params = Params::read(&item.sig)?;
        let doc = read_doc(&item.attrs, &item.sig, COMMAND_NEEDS_DOC)?;
        let ident = item.sig.ident.clone();
        let name = match args.name {
            Some(n) => n.value(),
            None => dashed(&ident),
        };
        args.meta.check_name(&name, "command")?;
        Ok(CommandFn {
            name,
            help: doc.help,
            details: doc.details,
            footer: doc.footer,
            aliases: args.meta.aliases,
            hide: args.meta.hide.is_some(),
            handle: handle_ident(&ident),
            cfg_attrs: cfg_attrs(&item.attrs),
            ident,
            arg_tys: params.arg_tys,
            bindings: params.bindings,
            slots: params.slots,
            ctx_ty: params.ctx_ty,
            item,
        })
    }
}

/// Reject the signatures a command cannot have, before looking at anything
/// else.
fn check_signature(sig: &Signature) -> Result<()> {
    if let Some(a) = &sig.asyncness {
        return Err(Error::new_spanned(
            a,
            "a #[command] cannot be `async`: hicli runs a command to completion, so drive the \
             runtime inside the body",
        ));
    }
    if let Some(u) = &sig.unsafety {
        return Err(Error::new_spanned(
            u,
            "a #[command] cannot be `unsafe`: give it a safe signature and keep the unsafe work \
             inside the body",
        ));
    }
    if let Some(abi) = &sig.abi {
        return Err(Error::new_spanned(
            abi,
            "a #[command] cannot have an `extern` ABI: it is called from Rust, by the command tree",
        ));
    }
    if let Some(v) = &sig.variadic {
        return Err(Error::new_spanned(
            v,
            "a #[command] cannot be variadic: each of its parameters is one argument struct",
        ));
    }
    let generics = &sig.generics;
    if !generics.params.is_empty() {
        return Err(Error::new_spanned(generics, GENERIC));
    }
    if let Some(w) = &generics.where_clause {
        return Err(Error::new_spanned(w, GENERIC));
    }
    Ok(())
}

/// The message for a generic or lifetime-bearing signature.
const GENERIC: &str = "a #[command] cannot be generic: its context type is fixed by the `&T` \
                       parameter and each argument is one concrete type deriving `Bpaf`";

/// The parameters of a command, split into arguments and the context.
struct Params {
    /// One argument type per owned parameter, as written, in declaration order.
    arg_tys: Vec<Type>,
    /// One closure binding per owned parameter, in declaration order.
    bindings: Vec<Ident>,
    /// Every parameter, in declaration order.
    slots: Vec<Slot>,
    /// The context type, if a `&T` parameter was declared.
    ctx_ty: Option<Type>,
}

impl Params {
    /// Classify each parameter: a `&T` is the context, an owned named type is
    /// an argument, anything else is an error.
    fn read(sig: &Signature) -> Result<Params> {
        let mut p = Params {
            arg_tys: Vec::new(),
            bindings: Vec::new(),
            slots: Vec::new(),
            ctx_ty: None,
        };
        for input in &sig.inputs {
            let typed = match input {
                FnArg::Receiver(r) => {
                    return Err(Error::new_spanned(
                        r,
                        "a #[command] is a free function: it cannot take `self`",
                    ))
                }
                FnArg::Typed(t) => t,
            };
            check_pattern(&typed.pat)?;
            match &*typed.ty {
                Type::Reference(r) => {
                    if r.mutability.is_some() {
                        return Err(Error::new_spanned(
                            &typed.ty,
                            "a #[command]'s context is shared, not mutable: take `&T`, and put \
                             any mutability inside `T`",
                        ));
                    }
                    if p.ctx_ty.is_some() {
                        return Err(Error::new_spanned(
                            typed,
                            "a #[command] takes at most one context parameter: the single `&T` \
                             argument that fixes its `Ctx`",
                        ));
                    }
                    p.ctx_ty = Some((*r.elem).clone());
                    p.slots.push(Slot::Ctx);
                }
                Type::Path(path) if path.qself.is_none() => {
                    // Kept as written, span and all: the expansion resolves it
                    // through `Arg`, and a type that is not one has to be
                    // reported at the parameter the author typed.
                    p.arg_tys.push(Type::Path(path.clone()));
                    let binding = format_ident!("a{}", p.bindings.len());
                    p.bindings.push(binding.clone());
                    p.slots.push(Slot::Arg(binding));
                }
                other => return Err(Error::new_spanned(other, NOT_A_NAMED_TYPE)),
            }
        }
        Ok(p)
    }
}

/// The message for an owned parameter whose type is not a named type.
const NOT_A_NAMED_TYPE: &str = "a #[command]'s arguments are named types implementing `Arg`; this \
                                is not a named type";

/// Reject anything but a plain binding: the generated closure has to call the
/// body by name, and a pattern has no name to call it by.
fn check_pattern(pat: &Pat) -> Result<()> {
    match pat {
        Pat::Ident(p) if p.subpat.is_none() && p.by_ref.is_none() => Ok(()),
        other => Err(Error::new_spanned(
            other,
            "a #[command]'s parameters are plain names: write `w: Words` and destructure in the \
             body (`w.0`)",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read `src` as a command and report `(command name, handle ident)`.
    fn names_of(src: &str) -> (String, String) {
        let f: ItemFn = syn::parse_str(src).expect("parses");
        let c = CommandFn::from_item(CommandArgs::default(), f).expect("is a command");
        (c.name, c.handle.to_string())
    }

    /// Read `src` as a command with `attr` as `#[command]`'s own arguments.
    fn command_with(attr: &str, src: &str) -> Result<CommandFn> {
        let args: CommandArgs = syn::parse_str(attr)?;
        let f: ItemFn = syn::parse_str(src).expect("parses");
        CommandFn::from_item(args, f)
    }

    /// The error `#[command(attr)]` raises on a plain documented function.
    fn rejection(attr: &str) -> String {
        match command_with(attr, "/// list things\nfn list() {}") {
            Ok(_) => panic!("`{attr}` was accepted"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn every_key_is_read() {
        let c = command_with(
            "name = \"show\", alias = \"ls\", hide, alias = \"l\"",
            "/// list things\nfn list() {}",
        )
        .expect("is a command");
        assert_eq!(c.name, "show");
        let aliases: Vec<String> = c.aliases.iter().map(LitStr::value).collect();
        assert_eq!(aliases, ["ls", "l"]);
        assert!(c.hide);

        let c = command_with("", "/// list things\nfn list() {}").expect("is a command");
        assert!(c.aliases.is_empty());
        assert!(!c.hide);
    }

    #[test]
    fn the_doc_comment_fills_the_footer() {
        let c = command_with(
            "",
            "/// list things\n///\n/// # Examples\n///\n/// ```sh\n/// demo list\n/// ```\nfn list() {}",
        )
        .expect("is a command");
        assert_eq!(c.footer.as_deref(), Some("Examples:\n  demo list"));
    }

    #[test]
    fn each_rejection_is_raised() {
        assert_eq!(rejection("help = \"x\""), UNKNOWN_ARG);
        assert_eq!(
            rejection("name = \"a\", name = \"b\""),
            "`name` is given twice"
        );
        assert_eq!(rejection("hide, hide"), "`hide` is given twice");
        assert_eq!(rejection("name = \"\""), "a name cannot be empty");
        assert_eq!(
            rejection("name = \"sh ow\""),
            "a name is one word: it cannot contain whitespace"
        );
        assert_eq!(
            rejection("name = \"--show\""),
            "a name is a command name, not a flag: it cannot start with `-`"
        );
        assert_eq!(rejection("alias = \"\""), "an alias cannot be empty");
        assert_eq!(
            rejection("alias = \"a b\""),
            "an alias is one word: it cannot contain whitespace"
        );
        assert_eq!(
            rejection("alias = \"-l\""),
            "an alias is a command name, not a flag: it cannot start with `-`"
        );
        assert_eq!(
            rejection("alias = \"ls\", alias = \"ls\""),
            "alias `ls` is given twice"
        );
        // the derived name …
        assert_eq!(
            rejection("alias = \"list\""),
            "alias `list` is this command's own name: an alias is another name"
        );
        // … and an explicit one
        assert_eq!(
            rejection("name = \"show\", alias = \"show\""),
            "alias `show` is this command's own name: an alias is another name"
        );
    }

    #[test]
    fn the_ident_becomes_the_name_with_dashes() {
        assert_eq!(
            names_of("/// add an admin\nfn add_admin() {}"),
            ("add-admin".to_string(), "__hicli_add_admin".to_string())
        );
    }

    #[test]
    fn a_raw_ident_loses_its_escape() {
        // `fn r#move` is the command `move`, and its handle is
        // `__hicli_move` — `r#` is Rust's spelling problem, not the user's.
        assert_eq!(
            names_of("/// move a thing\nfn r#move() {}"),
            ("move".to_string(), "__hicli_move".to_string())
        );
    }

    #[test]
    fn cfg_attributes_are_the_ones_copied_onto_the_handle() {
        let f: ItemFn = syn::parse_str(
            "/// only here\n#[cfg(unix)]\n#[cfg_attr(test, allow(unused))]\n#[inline]\nfn go() {}",
        )
        .expect("parses");
        let c = CommandFn::from_item(CommandArgs::default(), f).expect("is a command");
        let copied: Vec<String> = c
            .cfg_attrs
            .iter()
            .map(|a| a.path().get_ident().expect("a simple path").to_string())
            .collect();
        assert_eq!(copied, ["cfg", "cfg_attr"]);
    }
}
