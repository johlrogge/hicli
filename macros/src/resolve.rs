//! Which file a `mod x;` declaration means (ADR-005 §5).
//!
//! These are the Reference's rules for an outlined module, re-implemented
//! because a proc macro has to answer the question *before* rustc does:
//!
//! - an explicit `#[path = "…"]` on the declaration **wins**, and is relative
//!   to the declaring file's directory — even from a non-mod-rs file
//!   (spike 0.4);
//! - a **mod-rs** file (`mod.rs`, and a crate root) searches its **own**
//!   directory: `src/lib.rs` declaring `mod event;` means `src/event.rs` or
//!   `src/event/mod.rs`;
//! - a **non-mod-rs** file `x.rs` searches `x/`: `src/event.rs` declaring
//!   `mod audience;` means `src/event/audience.rs` or
//!   `src/event/audience/mod.rs`.
//!
//! The one thing a proc macro cannot know is whether the declaring file is a
//! **crate root that is not named `lib.rs` or `main.rs`** — `examples/demo.rs`
//! and `tests/cli.rs` are, and they are mod-rs files. The heuristic is *"if
//! there is no `demo/` directory beside `demo.rs`, it is a root"*, and the
//! other arrangement is kept as a second-choice candidate rather than guessed
//! at.
//!
//! `#[group]` on a file module then **emits `#[path]` only where rustc's own
//! rules would not load the file this module read** ([`Load::ByPath`]), and
//! leaves the declaration as written everywhere else ([`Load::Unaided`]). It
//! used to emit one on every declaration, which made the guess impossible to
//! get wrong — but rustc treats a file loaded through `#[path]` as a mod-rs
//! file, so every group file became one, and a plain `mod helper;` inside
//! `src/source.rs` resolved to `src/helper.rs` instead of
//! `src/source/helper.rs` (#25). A declaration is `Unaided` only when it is
//! provably what rustc does unprompted:
//!
//! - the declaring file is a `mod.rs`/`lib.rs`/`main.rs` and the file is
//!   beside it; or
//! - the declaring file `foo.rs` is itself loaded without a `#[path]` —
//!   [`ordinarily_loaded`], which finds a top-level, `#[path]`-free `mod foo;`
//!   in the file that owns `foo.rs`'s directory, recursively — and the file is
//!   in `foo/`;
//!
//! and, in both cases, the other arrangement (`x.rs` against `x/mod.rs`) is
//! absent, since both at once is E0761 to rustc. Everywhere else — a crate root
//! not named `lib.rs`/`main.rs` with a same-named directory, a group nested in
//! a `#[path]`-loaded file, the ambiguous pair — the `#[path]` stays, so a
//! wrong guess still cannot silently disagree with the compiler.
//!
//! Every path here is relative to the same anchor the declaring file is (see
//! [`crate::fs`]): that is what a diagnostic prints, and the emitted `#[path]`
//! is built as a string relative to the declaring file's directory, so an
//! absolute path can never reach the output.

use std::path::{Path, PathBuf};

use syn::Item;

use crate::fs::Fs;
use crate::shared::{path_attr, unraw_name};

/// A declaration that was resolved to a file.
pub(crate) struct Resolved {
    /// The file to read, relative to the anchor the declaring file was found
    /// under.
    pub(crate) file: PathBuf,
    /// The value for `#[path = "…"]`: relative to the declaring file's
    /// directory, `/`-separated.
    pub(crate) relative: String,
    /// Whether rustc, left to its own rules, loads `file` too.
    pub(crate) load: Load,
}

/// How rustc comes to load the file a declaration was resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Load {
    /// rustc's own rules find it: the declaration goes out as written.
    Unaided,
    /// A `#[path]` is needed — or this module cannot prove it is not.
    ByPath,
}

/// A declaration that was not resolved, and everywhere that was looked.
pub(crate) struct NotFound {
    /// The candidates, in the order they were tried, as a diagnostic spells
    /// them: relative to the same anchor the declaring file is.
    pub(crate) tried: Vec<String>,
}

/// The file `mod <name>;`, declared in `declaring`, means.
///
/// `explicit` is the author's own `#[path = "…"]`, if they wrote one.
pub(crate) fn resolve(
    fs: &dyn Fs,
    declaring: &Path,
    name: &str,
    explicit: Option<&str>,
) -> Result<Resolved, NotFound> {
    let dir = declaring.parent().unwrap_or_else(|| Path::new(""));

    // The author said where it is. There is one candidate and no rule to apply
    // — being wrong about it is their sentence to correct, not ours to guess
    // around.
    if let Some(p) = explicit {
        let file = dir.join(p);
        return if fs.is_file(&file) {
            Ok(Resolved {
                file,
                relative: p.to_string(),
                load: Load::ByPath,
            })
        } else {
            Err(NotFound {
                tried: vec![show(&file)],
            })
        };
    }

    let stem = declaring
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let beside = dir.join(&stem);
    // `mod.rs`/`lib.rs`/`main.rs` are mod-rs by name; anything else is one only
    // if it has no directory of its own to own submodules.
    let by_name = matches!(stem.as_str(), "mod" | "lib" | "main");
    let mod_rs = by_name || !fs.is_dir(&beside);

    // The prefixes, best first. A candidate under a directory that does not
    // exist cannot exist either, so it is not offered and not listed as tried.
    let here = String::new();
    let under = format!("{stem}/");
    let prefixes: Vec<String> = if mod_rs {
        let mut ps = vec![here];
        if fs.is_dir(&beside) {
            ps.push(under.clone());
        }
        ps
    } else {
        // The declaring file may still be a crate root with a same-named
        // directory beside it — `tests/cli.rs` next to `tests/cli/` — so its
        // own directory stays in the running.
        vec![under.clone(), here]
    };

    let mut tried: Vec<String> = Vec::new();
    for prefix in prefixes {
        let pair = [
            format!("{prefix}{name}.rs"),
            format!("{prefix}{name}/mod.rs"),
        ];
        for (i, relative) in pair.iter().enumerate() {
            let file = dir.join(relative);
            let shown = show(&file);
            if tried.contains(&shown) {
                continue;
            }
            if fs.is_file(&file) {
                // rustc's own prefix for this declaring file: beside a mod-rs
                // file by name, in `{stem}/` for a file rustc itself loaded,
                // and none it can be proved to agree with otherwise.
                let default = if by_name {
                    prefix.is_empty()
                } else {
                    prefix == under && ordinarily_loaded(fs, declaring)
                };
                let other = dir.join(&pair[1 - i]);
                let load = if default && !fs.is_file(&other) {
                    Load::Unaided
                } else {
                    Load::ByPath
                };
                return Ok(Resolved {
                    file,
                    relative: relative.clone(),
                    load,
                });
            }
            tried.push(shown);
        }
    }
    Err(NotFound { tried })
}

/// Whether rustc loads `file` (`D/foo.rs`) without any `#[path]`'s help.
///
/// It does when `D/mod.rs`, `D/lib.rs` or `D/main.rs` declares `foo` plainly,
/// or when `D.rs` — not itself named `mod`/`lib`/`main` — is ordinarily loaded
/// and declares `foo` plainly. In either case `D/foo/mod.rs` must be absent:
/// with both, the owner's `#[group]` would have emitted a `#[path]`, and a
/// plain declaration is E0761. `D` is shorter at every step, so the recursion
/// ends.
fn ordinarily_loaded(fs: &dyn Fs, file: &Path) -> bool {
    let Some(name) = file.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return false;
    };
    let dir = file.parent().unwrap_or_else(|| Path::new(""));
    if fs.is_file(&dir.join(&name).join("mod.rs")) {
        return false;
    }
    if ["mod.rs", "lib.rs", "main.rs"]
        .iter()
        .any(|owner| declares_plainly(fs, &dir.join(owner), &name))
    {
        return true;
    }
    let Some(dir_name) = dir.file_name().map(|s| s.to_string_lossy().into_owned()) else {
        return false;
    };
    if matches!(dir_name.as_str(), "mod" | "lib" | "main") {
        return false;
    }
    let owner = dir
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{dir_name}.rs"));
    fs.is_file(&owner) && ordinarily_loaded(fs, &owner) && declares_plainly(fs, &owner, &name)
}

/// Whether `owner` declares `mod <name>;` at its top level with no `#[path]`.
///
/// A file that cannot be read or parsed declares nothing this module can
/// prove.
fn declares_plainly(fs: &dyn Fs, owner: &Path, name: &str) -> bool {
    let Ok(text) = fs.read_to_string(owner) else {
        return false;
    };
    let Ok(parsed) = syn::parse_file(&text) else {
        return false;
    };
    parsed.items.iter().any(|item| match item {
        Item::Mod(m) => {
            m.content.is_none() && unraw_name(&m.ident) == name && path_attr(&m.attrs).is_none()
        }
        _ => false,
    })
}

/// A path as a diagnostic says it.
pub(crate) fn show(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MapFs;

    /// Resolve `mod <name>;` declared in `declaring`, against a tree of empty
    /// files: `(the file, the #[path] value)`.
    fn found(tree: &[&str], declaring: &str, name: &str) -> (String, String) {
        let fs = MapFs::touch(tree);
        let r = resolve(&fs, Path::new(declaring), name, None).unwrap_or_else(|e| {
            panic!(
                "`mod {name};` in `{declaring}` should resolve; tried {:?}",
                e.tried
            )
        });
        (show(&r.file), r.relative)
    }

    /// Everywhere resolution looked before giving up.
    fn tried(tree: &[&str], declaring: &str, name: &str, explicit: Option<&str>) -> Vec<String> {
        let fs = MapFs::touch(tree);
        match resolve(&fs, Path::new(declaring), name, explicit) {
            Ok(r) => panic!("`mod {name};` unexpectedly resolved to `{}`", show(&r.file)),
            Err(e) => e.tried,
        }
    }

    #[test]
    fn a_crate_root_searches_its_own_directory() {
        assert_eq!(
            found(&["src/main.rs", "src/event.rs"], "src/main.rs", "event"),
            ("src/event.rs".to_string(), "event.rs".to_string())
        );
    }

    #[test]
    fn a_directory_module_is_found_by_its_mod_rs() {
        assert_eq!(
            found(&["src/lib.rs", "src/event/mod.rs"], "src/lib.rs", "event"),
            ("src/event/mod.rs".to_string(), "event/mod.rs".to_string())
        );
    }

    #[test]
    fn a_non_mod_rs_file_searches_the_directory_named_after_it() {
        // `src/event.rs` with a `src/event/` beside it: the ordinary nested
        // case, and the one the three-level integration test is built on.
        assert_eq!(
            found(
                &["src/main.rs", "src/event.rs", "src/event/audience.rs"],
                "src/event.rs",
                "audience"
            ),
            (
                "src/event/audience.rs".to_string(),
                "event/audience.rs".to_string()
            )
        );
    }

    #[test]
    fn a_mod_rs_searches_beside_itself() {
        assert_eq!(
            found(&["src/x/mod.rs", "src/x/y.rs"], "src/x/mod.rs", "y"),
            ("src/x/y.rs".to_string(), "y.rs".to_string())
        );
    }

    #[test]
    fn a_crate_root_that_is_not_named_lib_or_main_is_still_a_root() {
        // The question a proc macro cannot ask the compiler: `examples/demo.rs`
        // is a crate root, so `mod event;` in it means `examples/event.rs`.
        // With no `examples/demo/` directory, the heuristic says so.
        assert_eq!(
            found(
                &["examples/demo.rs", "examples/event.rs"],
                "examples/demo.rs",
                "event"
            ),
            ("examples/event.rs".to_string(), "event.rs".to_string())
        );
    }

    #[test]
    fn a_root_with_a_directory_beside_it_prefers_the_directory() {
        // `tests/cli.rs` *is* a crate root, so rustc would say `tests/event.rs`
        // — but the author put the tree under `tests/cli/`, and the emitted
        // `#[path]` is what makes that the answer rather than a disagreement.
        assert_eq!(
            found(
                &["tests/cli.rs", "tests/cli/event.rs"],
                "tests/cli.rs",
                "event"
            ),
            ("tests/cli/event.rs".to_string(), "cli/event.rs".to_string())
        );
    }

    #[test]
    fn the_other_arrangement_is_the_second_choice() {
        // Same declaring file, same `cli/` directory — but the module is beside
        // it, the way a crate root's modules are. The second-choice candidate
        // finds it.
        assert_eq!(
            found(
                &["tests/cli.rs", "tests/cli/other.rs", "tests/event.rs"],
                "tests/cli.rs",
                "event"
            ),
            ("tests/event.rs".to_string(), "event.rs".to_string())
        );
    }

    #[test]
    fn an_explicit_path_wins_and_is_relative_to_the_declaring_file() {
        assert_eq!(
            found(
                &["src/main.rs", "src/event.rs", "src/other/dir.rs"],
                "src/main.rs",
                "event"
            )
            .0,
            "src/event.rs",
            "the default rule finds the obvious file"
        );
        let fs = MapFs::touch(&["src/main.rs", "src/event.rs", "src/other/dir.rs"]);
        let r = resolve(&fs, Path::new("src/main.rs"), "event", Some("other/dir.rs"))
            .unwrap_or_else(|_| panic!("the explicit path resolves"));
        assert_eq!(show(&r.file), "src/other/dir.rs");
        // Emitted back exactly as written: it already means the right thing.
        assert_eq!(r.relative, "other/dir.rs");
    }

    #[test]
    fn a_missing_file_lists_every_candidate() {
        assert_eq!(
            tried(&["src/main.rs"], "src/main.rs", "event", None),
            ["src/event.rs", "src/event/mod.rs"]
        );
    }

    #[test]
    fn a_missing_file_under_a_directory_module_lists_both_arrangements() {
        assert_eq!(
            tried(
                &["src/event.rs", "src/event/other.rs"],
                "src/event.rs",
                "audience",
                None
            ),
            [
                "src/event/audience.rs",
                "src/event/audience/mod.rs",
                "src/audience.rs",
                "src/audience/mod.rs",
            ]
        );
    }

    #[test]
    fn a_missing_explicit_path_names_only_what_the_author_asked_for() {
        assert_eq!(
            tried(&["src/main.rs"], "src/main.rs", "event", Some("nope.rs")),
            ["src/nope.rs"]
        );
    }

    // --- whether rustc finds it unaided (#25) ---------------------------------

    /// How rustc comes to load what `mod <name>;` in `declaring` resolved to,
    /// in a tree whose contents matter.
    fn load(tree: &[(&str, &str)], declaring: &str, name: &str) -> Load {
        let fs = MapFs::new(tree);
        resolve(&fs, Path::new(declaring), name, None)
            .unwrap_or_else(|e| panic!("`mod {name};` should resolve; tried {:?}", e.tried))
            .load
    }

    #[test]
    fn a_file_beside_a_crate_root_is_found_unaided() {
        let tree = [("src/main.rs", ""), ("src/event.rs", "")];
        assert_eq!(load(&tree, "src/main.rs", "event"), Load::Unaided);
    }

    #[test]
    fn a_directory_module_beside_a_crate_root_is_found_unaided() {
        let tree = [("src/lib.rs", ""), ("src/event/mod.rs", "")];
        assert_eq!(load(&tree, "src/lib.rs", "event"), Load::Unaided);
    }

    #[test]
    fn a_file_under_an_ordinarily_loaded_file_is_found_unaided() {
        for main in ["mod event;", "/// e\n#[group]\npub mod event;"] {
            let tree = [
                ("src/main.rs", main),
                ("src/event.rs", ""),
                ("src/event/audience.rs", ""),
            ];
            assert_eq!(
                load(&tree, "src/event.rs", "audience"),
                Load::Unaided,
                "with main.rs = {main:?}"
            );
        }
    }

    #[test]
    fn a_file_under_a_path_loaded_file_keeps_its_path() {
        let tree = [
            ("src/main.rs", "#[path = \"event.rs\"] mod event;"),
            ("src/event.rs", ""),
            ("src/event/audience.rs", ""),
        ];
        assert_eq!(load(&tree, "src/event.rs", "audience"), Load::ByPath);
    }

    #[test]
    fn a_root_with_its_own_directory_keeps_its_path_at_every_level() {
        let tree = [
            ("tests/cli.rs", "mod event;"),
            ("tests/cli/event.rs", ""),
            ("tests/cli/event/audience.rs", ""),
        ];
        assert_eq!(load(&tree, "tests/cli.rs", "event"), Load::ByPath);
        assert_eq!(load(&tree, "tests/cli/event.rs", "audience"), Load::ByPath);
    }

    #[test]
    fn a_three_level_chain_is_found_unaided() {
        let tree = [
            ("src/main.rs", "mod a;"),
            ("src/a.rs", "mod b;"),
            ("src/a/b.rs", ""),
            ("src/a/b/c.rs", ""),
        ];
        assert_eq!(load(&tree, "src/a/b.rs", "c"), Load::Unaided);
    }

    #[test]
    fn both_arrangements_at_once_keep_the_path() {
        let tree = [
            ("src/main.rs", ""),
            ("src/event.rs", ""),
            ("src/event/mod.rs", ""),
        ];
        assert_eq!(load(&tree, "src/main.rs", "event"), Load::ByPath);
    }

    #[test]
    fn a_declaration_inside_an_inline_module_proves_nothing() {
        let tree = [
            ("src/main.rs", "mod outer { mod event; }"),
            ("src/event.rs", ""),
            ("src/event/audience.rs", ""),
        ];
        assert_eq!(load(&tree, "src/event.rs", "audience"), Load::ByPath);
    }

    #[test]
    fn an_owner_that_does_not_parse_proves_nothing() {
        let tree = [
            ("src/main.rs", "mod event; fn ("),
            ("src/event.rs", ""),
            ("src/event/audience.rs", ""),
        ];
        assert_eq!(load(&tree, "src/event.rs", "audience"), Load::ByPath);
    }

    #[test]
    fn a_declaring_file_with_no_directory_resolves_beside_itself() {
        assert_eq!(
            found(&["main.rs", "event.rs"], "main.rs", "event"),
            ("event.rs".to_string(), "event.rs".to_string())
        );
    }
}
