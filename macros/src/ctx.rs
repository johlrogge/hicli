//! A group's context type (ADR-005 §4).
//!
//! A group's handle has to name the `Ctx` of the `Command` it returns, and a
//! `fn` cannot infer its own return type. Three rules decide it, in order:
//!
//! 1. `#[group(ctx = T)]`, when the author says so;
//! 2. otherwise the **first** `&T` in a depth-first, source-order walk of the
//!    children — a group's context is whatever its commands already take, so the
//!    old `ctx: Ctx;` header was ceremony for a type the code names anyway;
//! 3. otherwise the handle is **generic over `Ctx`**, exactly as a context-free
//!    `#[command]`'s is, so a context-free group mounts in any app.
//!
//! Only the *first* hit is examined. Two children naming two different context
//! types is not diagnosed here — `Ctx` and `crate::Ctx` are the same type with
//! different tokens, so a token-equality check would invent compile errors. The
//! first wins; the second fails as an ordinary `Command<A>` / `Command<B>`
//! mismatch, spanned at its own `push`.
//!
//! The walk crosses **file** boundaries too, through an [`Outline`]: a
//! `#[group] pub mod audience;` inside a file module resolves its own file,
//! relative to the file it was declared in, and the search continues there.
//! Nothing is diagnosed on that path — a child file that is missing or broken
//! raises its own error when *its* `#[group]` expands, and saying it
//! twice, once from a parent that only wanted to know a type name, helps
//! nobody. The walk simply stops and the handle comes out generic.

use std::path::PathBuf;

use quote::ToTokens;
use syn::{Error, Ident, Item, Path, Result, Type};

use crate::discover::{children_of, Child, ChildKind};
use crate::fs::Fs;
use crate::resolve::resolve as resolve_file;
use crate::shared::unraw_name;

/// What a group's handle names as its `Ctx`.
///
/// The type is boxed because `syn::Type` is a large enum and this one is not:
/// `Generic` carries nothing, and an unboxed `Named` would make every `Ctx`
/// as big as the biggest type syn can parse.
pub(crate) enum Ctx {
    /// A concrete type, either given or lifted from a child.
    Named(Box<Type>),
    /// Nothing named one: the handle is generic over `Ctx`.
    Generic,
}

/// Where an outlined child's file is, so the walk can follow it.
///
/// It is the declaring file plus the file system to read it with — the same
/// pair `#[group]` on a file module resolves its file from — and it deepens as the
/// walk descends: a child of `src/event.rs` resolves against `src/event.rs`,
/// and *its* children against whatever that resolved to.
pub(crate) struct Outline<'a> {
    /// The file system these paths are read from.
    fs: &'a dyn Fs,
    /// The file the children being walked were declared in.
    file: PathBuf,
    /// How many files deep the walk already is.
    ///
    /// A pair of files that declare each other is a module cycle rustc will
    /// reject — but rustc never gets the chance, because this walk runs first
    /// and would follow the cycle forever. A compile that hangs is worse than
    /// any diagnostic, so the walk simply stops going deeper; the context is
    /// then decided by what it did see, and the cycle is reported by the
    /// compiler as it always was.
    depth: u32,
}

/// How many files the context walk will descend through. Far past any real
/// tree: it is a stop, not a budget.
const MAX_DEPTH: u32 = 32;

impl<'a> Outline<'a> {
    /// The children of `file` are about to be walked.
    pub(crate) fn new(fs: &'a dyn Fs, file: PathBuf) -> Self {
        Outline { fs, file, depth: 0 }
    }

    /// The items of an outlined child's file, and an outline for *its*
    /// children. `None` when anything at all is in the way.
    fn open(&self, ident: &Ident, explicit: Option<&str>) -> Option<(Vec<Item>, Outline<'a>)> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        let found = resolve_file(self.fs, &self.file, &unraw_name(ident), explicit).ok()?;
        let text = self.fs.read_to_string(&found.file).ok()?;
        let items = syn::parse_file(&text).ok()?.items;
        let deeper = Outline {
            fs: self.fs,
            file: found.file,
            depth: self.depth + 1,
        };
        Some((items, deeper))
    }
}

/// Decide a group's context from its attribute argument and its children.
///
/// `outline` is `Some` for a group whose children were read out of a file, and
/// `None` for an inline one — an inline group refuses an outlined child without
/// an explicit `#[path]` (it would resolve against the wrong directory), so
/// there is nothing there to follow.
pub(crate) fn resolve(
    explicit: Option<Type>,
    children: &[Child],
    outline: Option<&Outline>,
) -> Result<Ctx> {
    if let Some(ty) = explicit {
        return Ok(Ctx::Named(Box::new(ty)));
    }
    Ok(match first_context(children, outline)? {
        Some(ty) => Ctx::Named(Box::new(ty)),
        None => Ctx::Generic,
    })
}

/// The first context type named by a child, depth-first in source order.
fn first_context(children: &[Child], outline: Option<&Outline>) -> Result<Option<Type>> {
    for child in children {
        match &child.kind {
            ChildKind::Command { ctx: Some(ty) } => {
                check_liftable(ty, &child.ident)?;
                return Ok(Some(ty.clone()));
            }
            ChildKind::Command { ctx: None } => {}
            // A nested group's commands are this group's commands too, one
            // module further down — and the walk stops at the first one that
            // names a type, wherever it is.
            ChildKind::Group { items } => {
                if let Some(ty) = first_context(&children_of(items), outline)? {
                    return Ok(Some(ty));
                }
            }
            // The body is in another file. Read it if there is an outline to
            // read it with, and keep walking one file further down.
            ChildKind::Outlined { path } => {
                let Some(outline) = outline else { continue };
                let Some((items, deeper)) = outline.open(&child.ident, path.as_deref()) else {
                    continue;
                };
                if let Some(ty) = first_context(&children_of(&items), Some(&deeper))? {
                    return Ok(Some(ty));
                }
            }
        }
    }
    Ok(None)
}

/// Can this type be written in the *parent* scope, where the handle lives?
///
/// The type is read inside the module and written outside it, so only a path
/// that means the same thing in both places can be lifted: a single ident (the
/// ordinary `Ctx`, in scope either side of a `use super::*`), a `crate::…` path
/// or a leading-`::` path. Anything module-relative — `super::Ctx`,
/// `types::Ctx`, `Self`, a qualified `<A as B>::C` — would silently resolve to
/// something else, or to nothing, one module up.
fn check_liftable(ty: &Type, owner: &Ident) -> Result<()> {
    if let Type::Path(p) = ty {
        if p.qself.is_none() && liftable(&p.path) {
            return Ok(());
        }
    }
    Err(Error::new_spanned(ty, relative(owner, ty)))
}

/// Is this path nameable from one module up?
fn liftable(path: &Path) -> bool {
    if path.leading_colon.is_some() {
        return true;
    }
    let mut segments = path.segments.iter();
    let Some(first) = segments.next() else {
        return false;
    };
    if first.ident == "crate" {
        return true;
    }
    // A single ident, and not one of the three that are relative by definition.
    segments.next().is_none() && !["self", "Self", "super"].iter().any(|k| first.ident == k)
}

/// The message for a context type that cannot be lifted (D11).
fn relative(owner: &Ident, ty: &Type) -> String {
    format!(
        "the context type of `{owner}` is written relative to its module (`{}`); name this \
         group's context with #[group(ctx = …)]",
        render(ty)
    )
}

/// A type as the author would have written it — `quote!` spaces every token,
/// and `super :: Ctx` in a diagnostic is not the text anybody typed.
///
/// Both sides of `::` are closed separately, so a leading one (`:: other::Ctx`)
/// is closed too.
fn render(ty: &Type) -> String {
    ty.to_token_stream()
        .to_string()
        .replace(" ::", "::")
        .replace(":: ", "::")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::fs::MapFs;

    /// Resolve the context of a module body, with an optional `ctx = …`.
    fn resolve_src(explicit: Option<&str>, src: &str) -> Result<Ctx> {
        let m: syn::ItemMod = syn::parse_str(src).expect("the module parses");
        let (_, items) = m.content.expect("an inline module");
        let explicit = explicit.map(|t| syn::parse_str::<Type>(t).expect("a type"));
        resolve(explicit, &children_of(&items), None)
    }

    /// The resolved context as source text, or `None` when it is generic.
    fn context_of(explicit: Option<&str>, src: &str) -> Option<String> {
        match resolve_src(explicit, src).expect("resolves") {
            Ctx::Named(ty) => Some(render(&ty)),
            Ctx::Generic => None,
        }
    }

    #[test]
    fn the_attribute_argument_wins() {
        assert_eq!(
            context_of(
                Some("crate::Ctx"),
                "mod event { /// c\n#[command] fn create(ctx: &Other) {} }"
            ),
            Some("crate::Ctx".to_string())
        );
    }

    #[test]
    fn the_first_child_that_names_one_wins() {
        assert_eq!(
            context_of(
                None,
                "mod event {
                     /// v\n#[command] fn version() {}
                     /// c\n#[command] fn create(t: Title, ctx: &Ctx) {}
                     /// r\n#[command] fn roster(ctx: &Later) {}
                 }"
            ),
            Some("Ctx".to_string())
        );
    }

    #[test]
    fn a_nested_groups_command_counts() {
        // The walk is depth-first: `audience`'s command is reached before
        // `roster`.
        assert_eq!(
            context_of(
                None,
                "mod event {
                     /// a\n#[group] mod audience { /// d\n#[command] fn define(c: &Deep) {} }
                     /// r\n#[command] fn roster(ctx: &Later) {}
                 }"
            ),
            Some("Deep".to_string())
        );
    }

    #[test]
    fn no_context_anywhere_is_generic() {
        assert_eq!(
            context_of(
                None,
                "mod tools {
                     /// v\n#[command] fn version() {}
                     /// a\n#[group] mod about { /// l\n#[command] fn license() {} }
                 }"
            ),
            None
        );
    }

    #[test]
    fn a_module_relative_context_is_refused() {
        let e = resolve_src(
            None,
            "mod event { /// c\n#[command] fn create(ctx: &super::Ctx) {} }",
        )
        .err()
        .expect("`super::Ctx` cannot be lifted");
        assert_eq!(
            e.to_string(),
            "the context type of `create` is written relative to its module (`super::Ctx`); \
             name this group's context with #[group(ctx = …)]"
        );
    }

    #[test]
    fn crate_and_absolute_paths_are_liftable() {
        assert_eq!(
            context_of(
                None,
                "mod event { /// c\n#[command] fn create(c: &crate::app::Ctx) {} }"
            ),
            Some("crate::app::Ctx".to_string())
        );
        assert_eq!(
            context_of(
                None,
                "mod event { /// c\n#[command] fn create(c: &::other::Ctx) {} }"
            ),
            Some("::other::Ctx".to_string())
        );
    }

    #[test]
    fn a_multi_segment_relative_path_is_refused() {
        // `types::Ctx` means `event::types::Ctx` from out here, which is either
        // a different type or no type at all.
        let e = resolve_src(
            None,
            "mod event { /// c\n#[command] fn create(c: &types::Ctx) {} }",
        )
        .err()
        .expect("`types::Ctx` cannot be lifted");
        assert!(e.to_string().contains("(`types::Ctx`)"), "{e}");
    }

    /// The context of `file`'s own children, walked across a real (in-memory)
    /// source tree.
    fn context_across(tree: &[(&str, &str)], file: &str) -> Option<String> {
        let fs = MapFs::new(tree);
        let src = fs
            .read_to_string(std::path::Path::new(file))
            .expect("the file is in the tree");
        let items = syn::parse_file(&src).expect("the file parses").items;
        let outline = Outline::new(&fs, std::path::PathBuf::from(file));
        match resolve(None, &children_of(&items), Some(&outline)).expect("resolves") {
            Ctx::Named(ty) => Some(render(&ty)),
            Ctx::Generic => None,
        }
    }

    #[test]
    fn a_command_two_files_down_names_the_context() {
        // `event.rs` declares nothing with a context of its own; `audience.rs`
        // does, and the walk crosses the file boundary to find it.
        assert_eq!(
            context_across(
                &[
                    ("src/event.rs", "/// a\n#[group] pub mod audience;"),
                    (
                        "src/event/audience.rs",
                        "/// d\n#[command] pub fn define(c: &crate::Ctx) {}",
                    ),
                ],
                "src/event.rs",
            ),
            Some("crate::Ctx".to_string())
        );
    }

    #[test]
    fn a_child_file_that_is_not_there_leaves_the_group_generic() {
        // Nothing is diagnosed: the missing file is `audience`'s own
        // `#[group]` to complain about, not this walk's.
        assert_eq!(
            context_across(
                &[(
                    "src/event.rs",
                    "/// c\n#[command] pub fn create() {}\n\
                     /// a\n#[group] pub mod audience;",
                )],
                "src/event.rs",
            ),
            None
        );
    }

    #[test]
    fn two_files_that_declare_each_other_terminate() {
        // A module cycle rustc will reject — after this macro has run. The walk
        // stops instead of following it forever.
        assert_eq!(
            context_across(
                &[
                    ("src/a.rs", "/// b\n#[group] pub mod b;"),
                    ("src/b.rs", "/// a\n#[group] pub mod a;"),
                ],
                "src/a.rs",
            ),
            None
        );
    }

    #[test]
    fn an_explicit_argument_silences_the_lift_check() {
        // The escape the diagnostic names has to actually work.
        assert_eq!(
            context_of(
                Some("Ctx"),
                "mod event { /// c\n#[command] fn create(ctx: &super::Ctx) {} }"
            ),
            Some("Ctx".to_string())
        );
    }
}
