//! A plain `mod helper;` inside a file group resolves as in any Rust file (#25).
//!
//! Each group file below declares an ordinary, un-annotated `mod helper;`, and
//! each helper says where it lives. rustc treats a file loaded through
//! `#[path]` as a mod-rs file, so a `#[path]` emitted on every group would send
//! `source.rs`'s `mod helper;` to `helper.rs` beside it instead of
//! `source/helper.rs` (E0583). Every command records its helper's `FROM`, so a
//! test sees which file was compiled.
//!
//! ```text
//! source              source.rs               (top level, x.rs)
//!   pull              helper: source/helper.rs
//!   jobs              source/jobs.rs          (nested, x.rs)
//!     list            helper: source/jobs/helper.rs
//!   deep              source/deep/mod.rs      (nested, x/mod.rs)
//!     dive            helper: source/deep/helper.rs
//! sink                sink/mod.rs             (top level, x/mod.rs)
//!   drain             helper: sink/helper.rs
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::pure;
use hicli::{app, cli, group, App, Failure};

/// The shared context. It records what ran.
pub struct Ctx {
    log: Rc<RefCell<Vec<String>>>,
}

impl Ctx {
    /// Record one line of evidence.
    pub fn note(&self, line: impl Into<String>) {
        self.log.borrow_mut().push(line.into());
    }
}

/// A fresh recording log.
fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

/// An `mk_ctx` recording into `log`. The globals are `()`.
fn ctx_from(log: &Rc<RefCell<Vec<String>>>) -> impl FnOnce(()) -> Result<Ctx, Failure> {
    let log = Rc::clone(log);
    move |()| Ok(Ctx { log })
}

/// What ran, in order.
fn ran(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    log.borrow().clone()
}

/// where things come from
#[group]
pub mod source;

/// where things go
#[group]
pub mod sink;

/// The whole tree, declared.
fn app() -> App<(), Ctx> {
    app! {
        globals: pure(()),
        commands: [source, sink],
    }
}

#[test]
fn a_helper_beside_a_top_level_file_group_is_in_its_directory() {
    let log = log();
    let code = app().run_with_args(cli! { source pull }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["source/helper.rs"]);
}

#[test]
fn a_helper_of_a_nested_file_group_is_in_its_directory() {
    let log = log();
    let code = app().run_with_args(cli! { source jobs list }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["source/jobs/helper.rs"]);
}

#[test]
fn a_helper_of_a_nested_directory_group_is_beside_its_mod_rs() {
    let log = log();
    let code = app().run_with_args(cli! { source deep dive }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["source/deep/helper.rs"]);
}

#[test]
fn a_helper_of_a_top_level_directory_group_is_beside_its_mod_rs() {
    let log = log();
    let code = app().run_with_args(cli! { sink drain }, ctx_from(&log));
    assert_eq!(code, 0);
    assert_eq!(ran(&log), ["sink/helper.rs"]);
}
