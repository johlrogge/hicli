//! Reading a `#[command]` function: the attribute's arguments, the fn's name,
//! its doc comment and its parameters.
//!
//! Everything the macro needs is decided here, so [`expand`](crate::expand)
//! never has to make a judgement call — and every rejection is raised with a
//! span pointing at the token the author has to change.

use quote::{format_ident, ToTokens};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, FnArg, Ident, ItemFn, LitStr, Pat, Result, Signature, Token, Type};

use crate::shared::{
    cfg_attrs, check_spelling, dashed, handle_ident, option_inner, parse_keys, read_doc, Arguments,
    CommandMeta, Spelling, COMMAND_NEEDS_DOC, DOUBLE_OPTION,
};

/// The message for an argument `#[command]` does not take.
const UNKNOWN_ARG: &str = "#[command] takes `name = \"...\"`, `alias = \"...\"` (repeatable), \
                           `hide`, `curried` and `default`";

/// The message for `curried` on a function with no owned parameter to curry.
const CURRIED_NOTHING: &str = "a `curried` command receives its group's argument as its first \
                               owned parameter, and this function has none: add a parameter of \
                               the group's `args` type first";

/// The message for a `curried` command whose first parameter is an `Option`
/// (ADR-019 §3).
const CURRIED_OPTIONAL: &str = "a `curried` command's first parameter is the value its group \
                                parsed, which is always there: write the group's `args` type, \
                                not `Option<…>`";

/// The message for `curried` and `default` on one command.
const CURRIED_DEFAULT: &str = "`curried` and `default` cannot be combined: a group that hands its \
                               commands an argument has no default command";

/// The message for a `default` command whose doc comment says more than its
/// first paragraph.
const DEFAULT_DOC: &str = "a `default` command is shown on its group's help page, so its doc \
                           comment is its help only, one paragraph: move the details and examples \
                           to the group's doc comment";

/// The message for `default` together with a key that spells or hides the
/// command's name: `name`, `alias` or `hide`, each with its own reason.
fn default_with(key: &str) -> String {
    if key == "hide" {
        return "`default` cannot be combined with `hide`: a default command is never listed, so \
                there is nothing to hide"
            .to_string();
    }
    format!(
        "`default` cannot be combined with `{key}`: a default command answers to no word, so it \
         takes no name or alias"
    )
}

/// What `#[command]` itself was given: `name = "..."`, `alias = "..."`
/// (repeatable), `hide`, `curried` and `default`, each optional.
#[derive(Default)]
pub(crate) struct CommandArgs {
    /// An explicit command name, overriding the ident.
    name: Option<LitStr>,
    /// The aliases and the visibility, read by the rules `#[group]` shares.
    meta: CommandMeta,
    /// Where `curried` was written, if it was: the first owned parameter is
    /// the group's argument, not one of the command's own.
    curried: Option<Ident>,
    /// Where `default` was written, if it was: the command runs when its group
    /// is invoked without naming a command.
    default: Option<Ident>,
}

impl CommandArgs {
    /// Whether `curried` was given — what a parent `#[group]` reads to sort
    /// this child into the commands that receive its argument.
    pub(crate) fn is_curried(&self) -> bool {
        self.curried.is_some()
    }

    /// Whether `default` was given — what a parent `#[group]` reads to refuse
    /// a second one, or one in a group that takes an argument.
    pub(crate) fn is_default(&self) -> bool {
        self.default.is_some()
    }
}

/// Read a bare-word key (`curried`, `default`) into `slot`, refusing a value
/// and a repeat.
fn bare_key(slot: &mut Option<Ident>, key: &Ident, input: ParseStream) -> Result<()> {
    if input.peek(Token![=]) {
        return Err(Error::new_spanned(
            key,
            format!("`{key}` takes no value: write `{key}`"),
        ));
    }
    if slot.is_some() {
        return Err(Error::new_spanned(key, format!("`{key}` is given twice")));
    }
    *slot = Some(key.clone());
    Ok(())
}

impl Parse for CommandArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut name: Option<LitStr> = None;
        let mut curried: Option<Ident> = None;
        let mut default: Option<Ident> = None;
        let meta = parse_keys(input, UNKNOWN_ARG, |key, input| {
            if key == "curried" {
                bare_key(&mut curried, key, input)?;
                return Ok(true);
            }
            if key == "default" {
                bare_key(&mut default, key, input)?;
                return Ok(true);
            }
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
        let args = CommandArgs {
            name,
            meta,
            curried,
            default,
        };
        args.check_combinations()?;
        Ok(args)
    }
}

impl CommandArgs {
    /// Refuse the keys that cannot go together with `default`, each at the
    /// key that conflicts with it.
    fn check_combinations(&self) -> Result<()> {
        let Some(default) = &self.default else {
            return Ok(());
        };
        if self.curried.is_some() {
            return Err(Error::new_spanned(default, CURRIED_DEFAULT));
        }
        if let Some(name) = &self.name {
            return Err(Error::new_spanned(name, default_with("name")));
        }
        if let Some(alias) = self.meta.aliases.first() {
            return Err(Error::new_spanned(alias, default_with("alias")));
        }
        if let Some(hide) = self.meta.hide {
            return Err(Error::new(hide, default_with("hide")));
        }
        Ok(())
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
    /// The help: the doc comment's first paragraph, joined into one line.
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
    /// Whether `default` was given: the handle ends in `.by_default()`.
    pub(crate) default: bool,
    /// With `curried`, the first owned parameter — the group's argument, which
    /// the command receives instead of parsing. It is *not* in `arg_tys` or
    /// `bindings`.
    pub(crate) curried: Option<CurriedParam>,
    /// The type of each owned parameter the command parses itself, **as
    /// written**, in declaration order. Each is resolved through `Arg` at
    /// expansion time, spanned where the author wrote it.
    pub(crate) arg_tys: Vec<Type>,
    /// The `# Arguments` help for each of those parameters, if it has one: the
    /// handle builds that parameter's parser with `ArgHelp::parser_with_help`.
    pub(crate) helps: Vec<Option<String>>,
    /// The closure binding for each of those parameters, in declaration order.
    pub(crate) bindings: Vec<Ident>,
    /// The original parameter order, for the call into the function.
    pub(crate) slots: Vec<Slot>,
    /// The context type `T` of the single `&T` parameter, if there is one.
    pub(crate) ctx_ty: Option<Type>,
    /// The function itself, re-emitted verbatim — name, signature, visibility,
    /// attributes (in their original order) and body, all untouched.
    pub(crate) item: ItemFn,
}

/// The parameter a `curried` command receives from its group.
pub(crate) struct CurriedParam {
    /// Its type, as written — the `T` of the handle's `Curried<T, Ctx>`.
    pub(crate) ty: Type,
    /// Its closure binding; it keeps its declaration-order name (`a0`).
    pub(crate) binding: Ident,
}

impl CommandFn {
    /// Read `item`, or explain why it cannot be a command.
    pub(crate) fn from_item(args: CommandArgs, item: ItemFn) -> Result<CommandFn> {
        check_signature(&item.sig)?;
        let mut params = Params::read(&item.sig)?;
        let mut curried_name: Option<Ident> = None;
        let curried = match &args.curried {
            None => None,
            Some(key) if params.arg_tys.is_empty() => {
                return Err(Error::new_spanned(key, CURRIED_NOTHING))
            }
            Some(_) if option_inner(&params.arg_tys[0]).is_some() => {
                return Err(Error::new_spanned(&params.arg_tys[0], CURRIED_OPTIONAL))
            }
            Some(_) => {
                curried_name = Some(params.names.remove(0));
                Some(CurriedParam {
                    ty: params.arg_tys.remove(0),
                    binding: params.bindings.remove(0),
                })
            }
        };
        let doc = read_doc(&item.attrs, &item.sig, COMMAND_NEEDS_DOC)?;
        if let Some(key) = &args.default {
            // ADR-011, now one paragraph: overrides on a default are deferred
            // with the rest of its doc comment.
            if doc.details.is_some() || doc.footer.is_some() || doc.arguments.is_some() {
                return Err(Error::new_spanned(key, DEFAULT_DOC));
            }
        }
        let helps = match &doc.arguments {
            None => vec![None; params.arg_tys.len()],
            Some(a) => overrides(
                a,
                &item.sig.ident,
                &params,
                curried_name.as_ref().zip(curried.as_ref().map(|c| &c.ty)),
            )?,
        };
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
            default: args.default.is_some(),
            curried,
            arg_tys: params.arg_tys,
            helps,
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
    /// The name each owned parameter was declared under, in declaration order
    /// — what an `# Arguments` entry names.
    names: Vec<Ident>,
    /// Every parameter, in declaration order.
    slots: Vec<Slot>,
    /// The context type, if a `&T` parameter was declared.
    ctx_ty: Option<Type>,
    /// The context parameter's name, if there is one.
    ctx_name: Option<Ident>,
}

/// The parameter-help overrides an `# Arguments` section gives, one slot per
/// parsed parameter (aligned with `arg_tys`), or why an entry names nothing
/// that can take one.
fn overrides(
    a: &Arguments,
    function: &Ident,
    params: &Params,
    curried: Option<(&Ident, &Type)>,
) -> Result<Vec<Option<String>>> {
    let mut helps: Vec<Option<String>> = vec![None; params.names.len()];
    let mut seen: Vec<&str> = Vec::new();
    for e in &a.entries {
        // `r#type` and `type` name the same parameter.
        let bare = e.name.strip_prefix("r#").unwrap_or(&e.name);
        if seen.contains(&bare) {
            return Err(Error::new_spanned(
                &e.at,
                format!("# Arguments gives `{}` twice", e.name),
            ));
        }
        seen.push(bare);
        if let Some(i) = params.names.iter().position(|n| n.unraw() == bare) {
            helps[i] = Some(e.help.clone());
            continue;
        }
        let message = if params.ctx_name.as_ref().is_some_and(|c| c.unraw() == bare) {
            format!(
                "# Arguments names `{}`, the context: it is not parsed, so it has no help",
                e.name
            )
        } else if let Some((_, ty)) = curried.filter(|(c, _)| c.unraw() == bare) {
            let ty = ty.to_token_stream().to_string().replace(' ', "");
            format!(
                "# Arguments names `{}`, which its group parses (`args = {ty}`): its help is the \
                 type's",
                e.name
            )
        } else {
            let names: Vec<String> = params
                .names
                .iter()
                .map(|n| format!("`{}`", n.unraw()))
                .collect();
            let f = function.unraw();
            match names.as_slice() {
                [] => format!(
                    "# Arguments names `{}`, which is not a parameter of `{f}`, and `{f}` parses \
                     no arguments",
                    e.name
                ),
                [one] => format!(
                    "# Arguments names `{}`, which is not a parameter of `{f}`; its parameters \
                     are {one}",
                    e.name
                ),
                [init @ .., last] => format!(
                    "# Arguments names `{}`, which is not a parameter of `{f}`; its parameters \
                     are {} and {last}",
                    e.name,
                    init.join(", ")
                ),
            }
        };
        return Err(Error::new_spanned(&e.at, message));
    }
    Ok(helps)
}

impl Params {
    /// Classify each parameter: a `&T` is the context, an owned named type is
    /// an argument, anything else is an error.
    fn read(sig: &Signature) -> Result<Params> {
        let mut p = Params {
            arg_tys: Vec::new(),
            bindings: Vec::new(),
            names: Vec::new(),
            slots: Vec::new(),
            ctx_ty: None,
            ctx_name: None,
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
            let name = match &*typed.pat {
                Pat::Ident(p) => p.ident.clone(),
                _ => unreachable!("check_pattern admits only a plain name"),
            };
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
                    p.ctx_name = Some(name);
                    p.slots.push(Slot::Ctx);
                }
                Type::Path(_) if option_inner(&typed.ty).and_then(option_inner).is_some() => {
                    return Err(Error::new_spanned(&typed.ty, DOUBLE_OPTION));
                }
                Type::Path(path) if path.qself.is_none() => {
                    // Kept as written, span and all: the expansion resolves it
                    // through `Arg`, and a type that is not one has to be
                    // reported at the parameter the author typed.
                    p.arg_tys.push(Type::Path(path.clone()));
                    p.names.push(name);
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
    use quote::ToTokens;

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

    /// The error `#[command(attr)]` raises on the function `src`.
    fn rejection_on(attr: &str, src: &str) -> String {
        match command_with(attr, src) {
            Ok(_) => panic!("`{attr}` on `{src}` was accepted"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn curried_splits_off_the_first_owned_parameter() {
        let c = command_with(
            "curried",
            "/// cancel a download\nfn cancel(ctx: &Ctx, s: SourceName, d: DownloadId) {}",
        )
        .expect("is a command");
        let curried = c.curried.expect("is curried");
        assert_eq!(curried.ty.to_token_stream().to_string(), "SourceName");
        assert_eq!(curried.binding.to_string(), "a0");
        // The rest keep their declaration-order names …
        let rest: Vec<String> = c.bindings.iter().map(ToString::to_string).collect();
        assert_eq!(rest, ["a1"]);
        let tys: Vec<String> = c
            .arg_tys
            .iter()
            .map(|t| t.to_token_stream().to_string())
            .collect();
        assert_eq!(tys, ["DownloadId"]);
        // … and the call order is untouched.
        assert_eq!(c.slots.len(), 3);
        assert!(!c.default);
    }

    #[test]
    fn without_curried_every_owned_parameter_is_parsed() {
        let c = command_with("", "/// go\nfn go(s: SourceName, d: DownloadId) {}")
            .expect("is a command");
        assert!(c.curried.is_none());
        assert_eq!(c.bindings.len(), 2);
    }

    #[test]
    fn default_is_read() {
        let c =
            command_with("default", "/// search\nfn search(q: Query) {}").expect("is a command");
        assert!(c.default);
        let args: CommandArgs = syn::parse_str("default").expect("parses");
        assert!(args.is_default() && !args.is_curried());
        let args: CommandArgs = syn::parse_str("curried, alias = \"c\"").expect("parses");
        assert!(args.is_curried() && !args.is_default());
    }

    #[test]
    fn each_curried_and_default_rejection_is_raised() {
        assert_eq!(rejection("curried, curried"), "`curried` is given twice");
        assert_eq!(rejection("default, default"), "`default` is given twice");
        assert_eq!(
            rejection("curried = true"),
            "`curried` takes no value: write `curried`"
        );
        assert_eq!(
            rejection("default = true"),
            "`default` takes no value: write `default`"
        );
        assert_eq!(rejection("curried, default"), CURRIED_DEFAULT);
        assert_eq!(rejection("default, curried"), CURRIED_DEFAULT);
        assert_eq!(
            rejection("default, name = \"x\""),
            "`default` cannot be combined with `name`: a default command answers to no word, so \
             it takes no name or alias"
        );
        assert_eq!(
            rejection("alias = \"x\", default"),
            "`default` cannot be combined with `alias`: a default command answers to no word, so \
             it takes no name or alias"
        );
        assert_eq!(
            rejection("default, hide"),
            "`default` cannot be combined with `hide`: a default command is never listed, so \
             there is nothing to hide"
        );
        // Optional twice is said once.
        assert_eq!(
            rejection_on(
                "",
                "/// list\nfn list(l: Option<std::option::Option<Limit>>) {}"
            ),
            DOUBLE_OPTION
        );
        // A curried value is always there: `Option` of it says nothing.
        assert_eq!(
            rejection_on(
                "curried",
                "/// sync\nfn sync(s: Option<SourceName>, ctx: &Ctx) {}"
            ),
            CURRIED_OPTIONAL
        );
        // … but a curried command's *other* parameters may be optional.
        assert!(command_with(
            "curried",
            "/// sync\nfn sync(s: SourceName, n: Option<Limit>, ctx: &Ctx) {}"
        )
        .is_ok());
        // `curried` on a function with nothing to curry — the context is
        // not an owned parameter.
        assert_eq!(rejection("curried"), CURRIED_NOTHING);
        assert_eq!(
            rejection_on("curried", "/// go\nfn go(ctx: &Ctx) {}"),
            CURRIED_NOTHING
        );
        // A default command's doc comment is one paragraph — which may run
        // over several lines.
        let c = command_with(
            "default",
            "/// search the library\n/// by text\nfn search() {}",
        )
        .expect("one paragraph");
        assert_eq!(c.help, "search the library by text");
        assert_eq!(
            rejection_on(
                "default",
                "/// search\n/// by text\n///\n/// More.\nfn search() {}"
            ),
            DEFAULT_DOC
        );
        assert_eq!(
            rejection_on("default", "/// search\n///\n/// More.\nfn search() {}"),
            DEFAULT_DOC
        );
        assert_eq!(
            rejection_on(
                "default",
                "/// search\n///\n/// # Examples\n///\n/// demo search\nfn search() {}"
            ),
            DEFAULT_DOC
        );
    }

    #[test]
    fn arguments_entries_land_on_their_parameters() {
        let c = command_with(
            "",
            "/// list\n///\n/// # Arguments\n/// * `by` - Order\n/// * `r#type` - Kind\n\
             fn list(d: Desc, by: SortOpt, r#type: Kind, ctx: &Ctx) {}",
        )
        .expect("is a command");
        assert_eq!(
            c.helps,
            [None, Some("Order".to_string()), Some("Kind".to_string())]
        );
        // A curried command's slots are its parsed parameters only.
        let c = command_with(
            "curried",
            "/// sync\n///\n/// # Arguments\n/// * `apply` - Really\n\
             fn sync(source: SourceName, apply: Apply, ctx: &Ctx) {}",
        )
        .expect("is a command");
        assert_eq!(c.helps, [Some("Really".to_string())]);
        let c = command_with("", "/// list\nfn list(by: SortOpt) {}").expect("is a command");
        assert_eq!(c.helps, [None]);
    }

    #[test]
    fn each_arguments_rejection_is_raised() {
        assert_eq!(
            rejection_on(
                "",
                "/// list\n/// # Arguments\n/// * `bye` - x\nfn list(by: SortOpt, ctx: &Ctx) {}"
            ),
            "# Arguments names `bye`, which is not a parameter of `list`; its parameters are `by`"
        );
        assert_eq!(
            rejection_on(
                "",
                "/// list\n/// # Arguments\n/// * `x` - x\nfn list(a: A, b: B, c: C) {}"
            ),
            "# Arguments names `x`, which is not a parameter of `list`; its parameters are `a`, \
             `b` and `c`"
        );
        assert_eq!(
            rejection_on(
                "",
                "/// list\n/// # Arguments\n/// * `x` - x\nfn list(ctx: &Ctx) {}"
            ),
            "# Arguments names `x`, which is not a parameter of `list`, and `list` parses no \
             arguments"
        );
        assert_eq!(
            rejection_on(
                "",
                "/// list\n/// # Arguments\n/// * `ctx` - x\nfn list(by: SortOpt, ctx: &Ctx) {}"
            ),
            "# Arguments names `ctx`, the context: it is not parsed, so it has no help"
        );
        assert_eq!(
            rejection_on(
                "curried",
                "/// sync\n/// # Arguments\n/// * `source` - x\n\
                 fn sync(source: SourceName, ctx: &Ctx) {}"
            ),
            "# Arguments names `source`, which its group parses (`args = SourceName`): its help \
             is the type's"
        );
        assert_eq!(
            rejection_on(
                "",
                "/// list\n/// # Arguments\n/// * `by` - x\n/// * `by` - y\nfn list(by: S) {}"
            ),
            "# Arguments gives `by` twice"
        );
        assert_eq!(
            rejection_on(
                "default",
                "/// search\n///\n/// # Arguments\n/// * `q` - x\nfn search(q: Query) {}"
            ),
            DEFAULT_DOC
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
