//! `group_mod!` takes module *declarations* and nothing else. A function put in
//! it by mistake — the obvious guess, since every other hicli macro takes a
//! list of commands — is told what the body is for.

use hicli::group_mod;

struct Ctx;

group_mod! {
    /// show status
    pub fn status(_ctx: &Ctx) {}
}

fn main() {}
