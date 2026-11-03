//! The body of the `tools` group, declared in `tests/group_files.rs` with an
//! author-written `#[path = "group_files/odds.rs"]`.
//!
//! The module is `tools` and the file is `odds.rs`: when the author says where
//! the file is, the macro reads that file and re-emits their `#[path]`
//! untouched rather than synthesising one of its own.
//!
//! Nothing in here names a context, so the `tools` handle comes out generic
//! over `Ctx` and the group mounts in any app.

use hicli::command;

/// print the version
#[command]
pub fn version() {}
