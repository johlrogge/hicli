//! The file system, behind a trait, so module resolution can be unit-tested.
//!
//! `group_mod!` is the one macro in this crate that reads something other than
//! its own input: it resolves a `mod x;` declaration to a file and parses it.
//! That makes the resolution rules (`crate::resolve`) the most intricate code
//! here *and* the hardest to reach from a test, because a test that needs a
//! directory tree on disk is a test nobody writes enough of.
//!
//! So the rules take an `&dyn Fs`. In a real expansion it is [`RealFs`]; in a
//! unit test it is [`MapFs`], a `BTreeMap` of path to contents, and a whole
//! source tree is three lines of a test.
//!
//! **Paths here are relative to an anchor** — a directory [`anchor`] found, and
//! [`RealFs`] carries as its `root` and joins every path onto. Under cargo that
//! anchor is the compiler's working directory, which is the *workspace* root
//! and not the package root (spike 0.1); under rust-analyzer's proc-macro
//! server it is neither, which is why the anchor is *searched for* rather than
//! assumed. Nothing above this module has to hold an absolute path, and so
//! nothing above it can accidentally emit one.

use std::io;
use std::path::{Path, PathBuf};

/// The three questions module resolution asks of a file system.
pub(crate) trait Fs {
    /// Is there a file at `path`?
    fn is_file(&self, path: &Path) -> bool;
    /// Is there a directory at `path`?
    fn is_dir(&self, path: &Path) -> bool;
    /// The contents of the file at `path`.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
}

/// The real one, anchored at whatever directory [`anchor`] found.
pub(crate) struct RealFs {
    /// What a relative path is relative to. Empty means the process's own
    /// working directory — which is the right answer for paths that are
    /// absolute already.
    root: PathBuf,
}

impl RealFs {
    /// Anchor at `root`.
    fn rooted(root: PathBuf) -> Self {
        RealFs { root }
    }

    /// `path`, anchored. An absolute `path` is already anchored, and `join`
    /// returns it unchanged.
    fn at(&self, path: &Path) -> PathBuf {
        self.root.join(path)
    }
}

/// The declaring file, anchored, and the file system to read it through.
///
/// `candidates` are what the compiler said the file is — `Span::local_file()`
/// then `Span::file()` — and this is the one place the environment is read, so
/// [`anchor`] itself stays testable.
///
/// A candidate that lands nowhere leaves the file `None` (D6) and the file
/// system anchored at the working directory, which is the answer that was
/// always used and costs nothing when no file is going to be read.
pub(crate) fn locate(candidates: &[PathBuf]) -> (RealFs, Option<PathBuf>) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    // Un-anchored: the search joins the roots itself, and asks about the whole
    // path each time.
    let search = RealFs::rooted(PathBuf::new());
    match anchor(&search, candidates, &cwd, &manifest_dir) {
        Some((root, file)) => (RealFs::rooted(root), Some(file)),
        None => (RealFs::rooted(cwd), None),
    }
}

/// Which directory the compiler's path for a source file is relative to.
///
/// Under cargo it is rustc's working directory, the workspace root (spike 0.1)
/// — but rust-analyzer's proc-macro server runs from somewhere else entirely
/// and reports the same path, so a single anchor is a guess that fails in an
/// editor while passing in CI. The search tries, for each candidate in turn:
///
/// 1. the path itself, when it is absolute — it names the file already
///    (trybuild's spans do);
/// 2. `cwd/path` — cargo;
/// 3. `manifest_dir/path`, then every parent of it up to the file system root —
///    which finds the workspace root from a member package, and is what makes
///    the answer independent of where the process was started.
///
/// The first that is a file wins, and comes back as the root it was found under
/// and the path relative to that root: keeping the *relative* half untouched is
/// what keeps a diagnostic readable (`src/event.rs`) and an emitted `#[path]`
/// the same under every front end.
pub(crate) fn anchor(
    fs: &dyn Fs,
    candidates: &[PathBuf],
    cwd: &Path,
    manifest_dir: &Path,
) -> Option<(PathBuf, PathBuf)> {
    for candidate in candidates {
        // An absolute path is its own anchor: joining it onto anything gives it
        // back, so there is nothing else to try for it.
        if candidate.is_absolute() {
            if fs.is_file(candidate) {
                return Some((PathBuf::new(), candidate.clone()));
            }
            continue;
        }
        for root in std::iter::once(cwd).chain(manifest_dir.ancestors()) {
            if fs.is_file(&root.join(candidate)) {
                return Some((root.to_path_buf(), candidate.clone()));
            }
        }
    }
    None
}

impl Fs for RealFs {
    fn is_file(&self, path: &Path) -> bool {
        self.at(path).is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.at(path).is_dir()
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(self.at(path))
    }
}

#[cfg(test)]
pub(crate) use map::MapFs;

#[cfg(test)]
mod map {
    use super::{Fs, Path};
    use std::collections::BTreeMap;
    use std::io;

    /// A source tree written inline in a test.
    ///
    /// A directory exists when something is under it, which is the only sense
    /// in which resolution asks: the mod-rs rule wants to know whether `x.rs`
    /// has an `x/` beside it, and an empty `x/` would not hold a module anyway.
    pub(crate) struct MapFs {
        /// Path to contents, `/`-separated.
        files: BTreeMap<String, String>,
    }

    impl MapFs {
        /// A tree of `(path, contents)` pairs.
        pub(crate) fn new(files: &[(&str, &str)]) -> Self {
            MapFs {
                files: files
                    .iter()
                    .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
                    .collect(),
            }
        }

        /// A tree of empty files: existence is all the test is about.
        pub(crate) fn touch(paths: &[&str]) -> Self {
            MapFs {
                files: paths
                    .iter()
                    .map(|p| ((*p).to_string(), String::new()))
                    .collect(),
            }
        }

        /// A path as this map spells it.
        fn key(path: &Path) -> String {
            path.to_string_lossy().replace('\\', "/")
        }
    }

    impl Fs for MapFs {
        fn is_file(&self, path: &Path) -> bool {
            self.files.contains_key(&Self::key(path))
        }

        fn is_dir(&self, path: &Path) -> bool {
            let prefix = format!("{}/", Self::key(path));
            self.files.keys().any(|k| k.starts_with(&prefix))
        }

        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            self.files.get(&Self::key(path)).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "No such file or directory (os error 2)",
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_knows_its_files_and_the_directories_they_imply() {
        let fs = MapFs::touch(&["src/main.rs", "src/event/audience.rs"]);
        assert!(fs.is_file(Path::new("src/main.rs")));
        assert!(!fs.is_file(Path::new("src/event.rs")));
        // `src/event` holds something, so it is a directory; `src/main` does
        // not, which is what tells the mod-rs rule that `main.rs` is a root.
        assert!(fs.is_dir(Path::new("src/event")));
        assert!(!fs.is_dir(Path::new("src/main")));
        assert!(fs.is_dir(Path::new("src")));
    }

    #[test]
    fn a_map_reads_back_what_was_put_in_it() {
        let fs = MapFs::new(&[("src/event.rs", "/// c\n#[command] fn create() {}")]);
        assert_eq!(
            fs.read_to_string(Path::new("src/event.rs"))
                .expect("is there"),
            "/// c\n#[command] fn create() {}"
        );
        assert!(fs.read_to_string(Path::new("src/nope.rs")).is_err());
    }

    // --- anchoring ------------------------------------------------------------

    /// Anchor `candidates` against `tree`, with `cwd` and `manifest` as the
    /// environment: `(the root, the file relative to it)`, as strings.
    fn anchored(
        tree: &[&str],
        candidates: &[&str],
        cwd: &str,
        manifest: &str,
    ) -> Option<(String, String)> {
        let fs = MapFs::touch(tree);
        let candidates: Vec<PathBuf> = candidates.iter().map(PathBuf::from).collect();
        anchor(&fs, &candidates, Path::new(cwd), Path::new(manifest))
            .map(|(root, file)| (root.display().to_string(), file.display().to_string()))
    }

    #[test]
    fn an_absolute_candidate_anchors_itself() {
        // Nothing to join it onto: under trybuild the compiler reports an
        // absolute path, and it already names the file.
        assert_eq!(
            anchored(
                &["/w/tests/ui/case.rs"],
                &["/w/tests/ui/case.rs"],
                "/somewhere/else",
                "/w"
            ),
            Some((String::new(), "/w/tests/ui/case.rs".to_string()))
        );
    }

    #[test]
    fn the_working_directory_is_tried_first() {
        // The cargo case (spike 0.1): rustc's cwd is the workspace root and the
        // reported path is relative to it. The root is what a diagnostic is
        // relative to, so the file stays `examples/files/main.rs`.
        assert_eq!(
            anchored(
                &["/w/examples/files/main.rs"],
                &["examples/files/main.rs"],
                "/w",
                "/w/crates/hicli-macros"
            ),
            Some(("/w".to_string(), "examples/files/main.rs".to_string()))
        );
    }

    #[test]
    fn the_manifest_directory_is_tried_when_the_working_directory_is_elsewhere() {
        // rust-analyzer's proc-macro server does not run from the workspace
        // root; `CARGO_MANIFEST_DIR` still points into the tree.
        assert_eq!(
            anchored(
                &["/w/examples/files/main.rs"],
                &["examples/files/main.rs"],
                "/proc/self",
                "/w"
            ),
            Some(("/w".to_string(), "examples/files/main.rs".to_string()))
        );
    }

    #[test]
    fn a_parent_of_the_manifest_directory_is_tried_too() {
        // A member package: the manifest is `/w/crates/thing`, and the path the
        // compiler reported is relative to the workspace root above it.
        assert_eq!(
            anchored(
                &["/w/examples/files/main.rs"],
                &["examples/files/main.rs"],
                "/proc/self",
                "/w/crates/thing"
            ),
            Some(("/w".to_string(), "examples/files/main.rs".to_string()))
        );
    }

    #[test]
    fn the_first_candidate_that_lands_anywhere_wins() {
        // `local_file()` first, `file()` after it: a remapped first candidate
        // does not stop the second from being tried.
        assert_eq!(
            anchored(
                &["/w/src/main.rs"],
                &["/remapped/src/main.rs", "src/main.rs"],
                "/w",
                "/w"
            ),
            Some(("/w".to_string(), "src/main.rs".to_string()))
        );
    }

    #[test]
    fn a_path_that_lands_nowhere_is_not_anchored() {
        assert_eq!(
            anchored(&["/w/src/main.rs"], &["src/other.rs"], "/w", "/w/crates/x"),
            None
        );
        // … and neither is nothing at all.
        assert_eq!(anchored(&["/w/src/main.rs"], &[], "/w", "/w"), None);
    }
}
