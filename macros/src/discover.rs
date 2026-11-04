//! Which items in a module body are a group's children.
//!
//! **A child is a child because it is in the module** (ADR-005 §3): there is no
//! list to keep in step with the code, so a command that was written cannot be
//! a command that was never registered. Two shapes qualify, in source order:
//!
//! - a `fn` marked `#[command]`,
//! - a `mod` marked `#[group]` — inline (`mod x { … }`) or a file (`mod x;`).
//!
//! Everything else in the body — argument structs, `impl` blocks, an ordinary
//! `mod`, `#[cfg(test)] mod tests`, a macro invocation — is skipped, because
//! none of them carries the mark.
//!
//! The mark is matched on an attribute path's **last segment**, so
//! `#[command]`, `#[hicli::command]` and `#[crate::command]` all count. That is
//! a deliberate over-approximation: a proc macro sees tokens, not resolved
//! paths, and the failure mode of guessing wrong is a missing-function error
//! naming `__hicli_x`, not silence.

use syn::parse::Parse;
use syn::{Attribute, FnArg, Ident, Item, Meta, Signature, Type};

use crate::group::GroupArgs;
use crate::parse::CommandArgs;
use crate::shared::{cfg_attrs, marks, path_attr};

/// One discovered child of a group.
pub(crate) struct Child {
    /// The item's ident — the group's handle calls `__hicli_<ident>` inside
    /// the module, so this is a name, not a command-line spelling.
    pub(crate) ident: Ident,
    /// The child's own `#[cfg]` / `#[cfg_attr]`, copied onto its `push`
    /// statement so a configured-out child is a registration that is not there.
    pub(crate) cfgs: Vec<Attribute>,
    /// What kind of child it is — which is also how deep the context walk can
    /// look.
    pub(crate) kind: ChildKind,
}

/// The shapes a child comes in: a command, or a group whose body is inline or
/// in a file.
pub(crate) enum ChildKind {
    /// A `#[command]` function, with the type of its `&T` parameter if it has
    /// one.
    Command {
        /// The context type the command names, if any.
        ctx: Option<Type>,
        /// Whether it is `#[command(curried)]`: it receives its group's
        /// argument.
        curried: bool,
        /// Whether it is `#[command(default)]`.
        default: bool,
    },
    /// A nested `#[group]` module, with its body — which the context walk
    /// recurses into.
    Group {
        /// The nested module's items.
        items: Vec<Item>,
        /// Whether it is `#[group(args = T)]`: it takes an argument of its own.
        args: bool,
        /// Whether it is `#[group(curried = T)]`: it receives its group's
        /// argument.
        curried: bool,
    },
    /// A `#[group] mod x;`: the body is in another file.
    ///
    /// Discovery does not read it — the group's handle reaches exactly one
    /// level down, and this child's own `#[group]` writes its handle. Only
    /// the *context* walk goes further (`crate::ctx::Outline`), and it needs
    /// the one thing the declaration says about where the file is.
    Outlined {
        /// The author's own `#[path = "…"]`, if they wrote one.
        path: Option<String>,
        /// Whether it is `#[group(args = T)]`.
        args: bool,
        /// Whether it is `#[group(curried = T)]`.
        curried: bool,
    },
}

impl ChildKind {
    /// Does this child receive its group's argument — a `#[command(curried)]`
    /// or a `#[group(curried = T)]`?
    pub(crate) fn is_curried(&self) -> bool {
        match self {
            ChildKind::Command { curried, .. }
            | ChildKind::Group { curried, .. }
            | ChildKind::Outlined { curried, .. } => *curried,
        }
    }

    /// Is this child a `#[command(default)]`?
    pub(crate) fn is_default(&self) -> bool {
        matches!(self, ChildKind::Command { default: true, .. })
    }

    /// Is this child a `#[group(args = T)]`?
    pub(crate) fn takes_args(&self) -> bool {
        match self {
            ChildKind::Command { .. } => false,
            ChildKind::Group { args, .. } | ChildKind::Outlined { args, .. } => *args,
        }
    }
}

/// Every child of a module body, in source order.
pub(crate) fn children_of(items: &[Item]) -> Vec<Child> {
    let mut children = Vec::new();
    for item in items {
        match item {
            Item::Fn(f) if marked(&f.attrs, "command") => {
                let args: CommandArgs = mark_args(&f.attrs, "command");
                children.push(Child {
                    ident: f.sig.ident.clone(),
                    cfgs: cfg_attrs(&f.attrs),
                    kind: ChildKind::Command {
                        ctx: context_of(&f.sig),
                        curried: args.is_curried(),
                        default: args.is_default(),
                    },
                });
            }
            Item::Mod(m) if marked(&m.attrs, "group") => {
                let g: GroupArgs = mark_args(&m.attrs, "group");
                let (args, curried) = (g.takes_args(), g.is_curried());
                children.push(Child {
                    ident: m.ident.clone(),
                    cfgs: cfg_attrs(&m.attrs),
                    kind: match &m.content {
                        Some((_, items)) => ChildKind::Group {
                            items: items.clone(),
                            args,
                            curried,
                        },
                        None => ChildKind::Outlined {
                            path: path_attr(&m.attrs),
                            args,
                            curried,
                        },
                    },
                });
            }
            _ => {}
        }
    }
    children
}

/// Does any of `attrs` end in `name`?
fn marked(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| marks(a, name))
}

/// The arguments of the attribute marking a child, read **leniently**.
///
/// A bare `#[command]` has none. Arguments that do not parse read as none
/// too: the child's own attribute reports them, with the span to say it at, when
/// it expands — saying it twice, once from a parent that only wanted to know
/// which keys were there, helps nobody.
fn mark_args<T: Parse + Default>(attrs: &[Attribute], name: &str) -> T {
    let Some(attr) = attrs.iter().find(|a| marks(a, name)) else {
        return T::default();
    };
    match &attr.meta {
        Meta::List(list) => syn::parse2(list.tokens.clone()).unwrap_or_default(),
        Meta::Path(_) | Meta::NameValue(_) => T::default(),
    }
}

/// The type of a signature's first `&T` parameter, if it has one.
///
/// No validation happens here: `#[command]` itself rejects a second context or a
/// `&mut T` when it expands, with the span to say it at. Discovery only needs
/// to know what type the group would be lifting.
fn context_of(sig: &Signature) -> Option<Type> {
    sig.inputs.iter().find_map(|input| match input {
        FnArg::Typed(t) => match &*t.ty {
            Type::Reference(r) => Some((*r.elem).clone()),
            _ => None,
        },
        FnArg::Receiver(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    /// The children of a module body written as source.
    fn children(src: &str) -> Vec<Child> {
        let m: syn::ItemMod = syn::parse_str(src).expect("the module parses");
        let (_, items) = m.content.expect("an inline module");
        children_of(&items)
    }

    /// The idents of the children of a module body, in order.
    fn names(src: &str) -> Vec<String> {
        children(src).iter().map(|c| c.ident.to_string()).collect()
    }

    #[test]
    fn children_are_in_source_order() {
        assert_eq!(
            names(
                "mod event {
                     /// create\n#[command] pub fn create(ctx: &Ctx) {}
                     /// audience\n#[group] pub mod audience { /// d\n#[command] fn d(c: &Ctx) {} }
                     /// roster\n#[command] pub fn roster(ctx: &Ctx) {}
                 }"
            ),
            ["create", "audience", "roster"]
        );
    }

    #[test]
    fn an_unmarked_item_is_not_a_child() {
        // The argument structs, the `impl`s and the plain modules a group's body
        // is mostly made of.
        assert_eq!(
            names(
                "mod event {
                     use super::*;
                     pub struct Title(String);
                     impl Title { fn new() {} }
                     pub mod helpers { pub fn h() {} }
                     pub fn plain() {}
                     /// create\n#[command] pub fn create(ctx: &Ctx) {}
                 }"
            ),
            ["create"]
        );
    }

    #[test]
    fn the_test_module_is_not_a_child() {
        // The one that would hurt: every consumer has `#[cfg(test)] mod tests`,
        // and it is skipped for the ordinary reason — it is not a `#[group]`.
        assert_eq!(
            names(
                "mod event {
                     /// create\n#[command] pub fn create(ctx: &Ctx) {}
                     #[cfg(test)] mod tests { #[test] fn t() {} }
                 }"
            ),
            ["create"]
        );
    }

    #[test]
    fn a_qualified_attribute_path_is_the_same_mark() {
        assert_eq!(
            names(
                "mod event {
                     /// create\n#[hicli::command] pub fn create(ctx: &Ctx) {}
                     /// audience\n#[hicli::group] pub mod audience {}
                 }"
            ),
            ["create", "audience"]
        );
    }

    #[test]
    fn a_nested_group_carries_its_body() {
        let cs = children(
            "mod event {
                 /// audience\n#[group] pub mod audience { /// d\n#[command] fn define(c: &Ctx) {} }
             }",
        );
        let ChildKind::Group { items, .. } = &cs[0].kind else {
            panic!("`audience` is a group child");
        };
        assert_eq!(
            children_of(items)
                .iter()
                .map(|c| c.ident.to_string())
                .collect::<Vec<_>>(),
            ["define"]
        );
    }

    #[test]
    fn a_file_module_has_no_body_to_recurse_into() {
        let cs = children("mod event { /// audience\n#[group] pub mod audience; }");
        assert!(matches!(cs[0].kind, ChildKind::Outlined { path: None, .. }));
    }

    #[test]
    fn a_commands_curried_and_default_keys_are_read() {
        let cs = children(
            "mod source {
                 /// s\n#[command(curried)] pub fn sync(s: SourceName, ctx: &Ctx) {}
                 /// c\n#[hicli::command(alias = \"x\", curried)] pub fn cancel(s: SourceName) {}
                 /// l\n#[command(default)] pub fn list(ctx: &Ctx) {}
                 /// p\n#[command] pub fn plain(ctx: &Ctx) {}
             }",
        );
        let flags: Vec<(bool, bool)> = cs
            .iter()
            .map(|c| (c.kind.is_curried(), c.kind.is_default()))
            .collect();
        assert_eq!(
            flags,
            [(true, false), (true, false), (false, true), (false, false)]
        );
    }

    #[test]
    fn a_groups_args_and_curried_keys_are_read_in_both_forms() {
        let cs = children(
            "mod top {
                 /// a\n#[group(args = SourceName)] pub mod source { /// l\n#[command] fn l() {} }
                 /// b\n#[group(curried = SourceName, ctx = Ctx)] pub mod jobs;
                 /// c\n#[group] pub mod plain;
             }",
        );
        let flags: Vec<(bool, bool)> = cs
            .iter()
            .map(|c| (c.kind.takes_args(), c.kind.is_curried()))
            .collect();
        assert_eq!(flags, [(true, false), (false, true), (false, false)]);
        assert!(matches!(cs[0].kind, ChildKind::Group { .. }));
        assert!(matches!(cs[1].kind, ChildKind::Outlined { .. }));
    }

    #[test]
    fn arguments_that_do_not_parse_read_as_unmarked() {
        // The child's own attribute reports these; discovery only sees a
        // plain child.
        let cs = children(
            "mod top {
                 /// s\n#[command(curried, bogus)] pub fn sync(s: SourceName) {}
                 /// d\n#[command(curried, default)] pub fn both(s: SourceName) {}
                 /// g\n#[group(default)] pub mod g { /// l\n#[command] fn l() {} }
             }",
        );
        for c in &cs {
            assert!(!c.kind.is_curried(), "`{}` read as curried", c.ident);
            assert!(!c.kind.is_default(), "`{}` read as default", c.ident);
            assert!(!c.kind.takes_args(), "`{}` read as taking args", c.ident);
        }
    }

    #[test]
    fn a_declarations_own_path_attribute_is_carried() {
        // The context walk resolves this child's file itself, so the one thing
        // the declaration says about where it is has to survive discovery.
        let cs = children(
            "mod event {
                 /// audience\n#[group] #[path = \"other/a.rs\"] pub mod audience;
             }",
        );
        let ChildKind::Outlined { path, .. } = &cs[0].kind else {
            panic!("`audience` is an outlined child");
        };
        assert_eq!(path.as_deref(), Some("other/a.rs"));
    }

    #[test]
    fn a_macro_invocation_is_not_a_child() {
        // Whatever a macro would expand to, discovery sees tokens it does not
        // read: a child is a marked `fn` or `mod`, nothing else.
        assert_eq!(
            names(
                "mod event {
                     some_macro! { /// audience\n pub mod audience; }
                     /// create\n#[command] pub fn create(ctx: &Ctx) {}
                 }"
            ),
            ["create"]
        );
    }

    #[test]
    fn a_childs_cfg_attributes_are_kept() {
        let cs = children(
            "mod event {
                 /// create\n#[cfg(unix)]\n#[inline]\n#[command] pub fn create(ctx: &Ctx) {}
             }",
        );
        let cfgs: Vec<String> = cs[0]
            .cfgs
            .iter()
            .map(|a| a.path().get_ident().expect("a simple path").to_string())
            .collect();
        assert_eq!(cfgs, ["cfg"]);
    }

    #[test]
    fn a_commands_context_type_is_read_from_its_reference_parameter() {
        let cs = children(
            "mod event {
                 /// create\n#[command] pub fn create(t: Title, ctx: &crate::Ctx) {}
                 /// version\n#[command] pub fn version() {}
             }",
        );
        let ChildKind::Command { ctx: Some(ty), .. } = &cs[0].kind else {
            panic!("`create` names a context");
        };
        assert_eq!(ty.to_token_stream().to_string(), "crate :: Ctx");
        assert!(matches!(cs[1].kind, ChildKind::Command { ctx: None, .. }));
    }
}
