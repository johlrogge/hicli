# ADR-005 build plan (working document, not part of the crate)

Architect's plan for "a verb is the function; a group is a module", 2026-09-17.
Read `docs/adr/005-verbs-are-the-function.md` and `docs/sketches/moniker.rs` first.

## Step 0 — spike results (2026-09-17, scratchpad crate `spike2`)

| # | Question | Answer | Consequence |
|---|---|---|---|
| 0.1 | `Span::local_file()` / `file()` | Both work on stable 1.97 and return a path **relative to rustc's cwd, which cargo sets to the workspace root** (not the package root). `current_dir()` inside the macro = workspace root. | Anchor with `std::env::current_dir()?.join(local_file)`; **never** `CARGO_MANIFEST_DIR` (silently wrong in a workspace). rust-analyzer untested — needs a manual editor check. |
| 0.2 | `#[proc_macro_attribute] group` + `#[proc_macro] group` | **E0428** in one crate; re-exporting from two crates under one name is **E0252** ("must be defined only once in the macro namespace"). | The file-module form needs its own name: `group_mod!` (provisional). |
| 0.3 | `syn::parse_file` inside a proc macro | Parses inner attrs, raw/byte strings, nested and outlined mods. **Every span from a parsed file collapses to `Span::call_site()`**; a `respan` helper is a no-op. | Diagnostics about a scanned file must synthesise `path:line` text; they always underline the invocation. |
| 0.4 | Macro-emitted `#[path = ".."] mod x;` | Resolves; `#[path]` on an outlined mod is relative to the **declaring file's directory** even from a non-mod-rs file. Plain `mod x;` from `src/nested.rs` → `src/nested/x.rs`. | Emit `#[path]` for resolved declarations as designed. |

## Step 1 — `#[verb]` untouched fn + `commands!` proc macro — DONE 2026-09-17

Fn re-emitted verbatim; `__verbandi_<ident>` handle (`pub`, `#[doc(hidden)]`,
`#[allow(unreachable_pub)]`, cfgs copied); `commands!` rewrites bare paths;
`app!` forwards `$($c:tt)*`; raw-ident name bug fixed. 13 UI cases, none changed.

## Step 2 — `#[group]` on inline modules; delete list-form `group!` — DONE 2026-09-17

`macros/src/{shared,discover,ctx,group}.rs`; `#[group]` attribute re-exported beside
`fn group` (different namespaces). Diagnostics D1, D2, D6 (file-module arm, gated by
rustc on 1.97 but the macro's hint still prints), D9–D11 recorded in `tests/ui/`.
`tests/groups.rs` (16 tests). Deleted `src/macro_support.rs`, `__group_fn!`, the
temporary `__group_handle`. `commands![(c)]` drops the parentheses on emission so the
documented local-value escape does not warn.

## Step 3 — `group_mod!` for file modules — DONE 2026-09-17

- `macros/src/fs.rs` (`trait Fs` + `RealFs` + in-memory test `Fs`), `macros/src/resolve.rs`
  (Reference rules: mod-rs vs non-mod-rs, `foo.rs`/`foo/mod.rs`, explicit `#[path]` wins;
  a crate root not named `lib.rs`/`main.rs` is handled by "if `dir/<stem>/` does not exist,
  assume mod-rs"), `macros/src/group.rs::file_form`, `#[proc_macro] group_mod` in `lib.rs`.
- Declaring file: `Span::call_site().local_file()` (fallback `file()`), joined onto
  `current_dir()` (spike 0.1). No `proc_macro::` type outside `lib.rs`.
- Emit `#[path = "<relative>"]` on every resolved declaration; never an absolute path.
- `#[group]`'s `ItemMod { semi }` arm routes here (unreachable on 1.97; the 1.99 alias). On
  1.97 rustc rejects `#[group] pub mod x;` with its own E0658 before the macro would ever
  raise D3 — confirmed, not just anticipated.
- Diagnostics D3 (not found, lists candidates, notes the top-level-only limitation), D4
  (parse failure), D5 (IO), D6 (source path unknown), D7 (not a mod decl), D8 (inline body).
- Tests: `resolve` unit tests against the in-memory `Fs`; `tests/group_files.rs` (11 cases) +
  `tests/group_files/event.rs` + `event/audience.rs` (three levels, a cfg'd-out verb, an
  inline `#[group]` inside a file module); trybuild `group_mod_file_missing`,
  `group_mod_not_a_mod`, `group_mod_inline_body` (22 cases across the whole trybuild suite,
  counting the earlier `#[verb]`/`#[group]` cases). `examples/files/{main,event}.rs` with a
  `[[example]]` in `Cargo.toml` and a `just demo-files` recipe.
- Source-file anchoring (post-spike fix, same day): rust-analyzer's proc-macro server does
  not run with the workspace root as cwd, so `macros/src/fs.rs::anchor` searches, in order:
  the path itself if absolute, `current_dir()/p`, `CARGO_MANIFEST_DIR/p`, then each ancestor
  of the manifest dir. The winning directory becomes the `Fs` root, so emitted `#[path]`
  values and diagnostics are unchanged under cargo. Editor verification still manual.
- Known limitation: `group_mod!` must be at a file's top level (a proc macro cannot see an
  enclosing inline module path).
- Final counts: 92 unit tests inside `macros/src` (across `resolve`, `commands`, `fs`,
  `parse`, `expand`, `shared`, `group`, `ctx`, `discover`), 11 in `tests/group_files.rs`, 22
  `trybuild` UI cases.
- Decisions made implementing the plan, beyond the numbered list below:
  - A secondary resolution candidate (the "declaring file might itself be a crate root with
    a same-named directory beside it" case) is skipped, and not listed as tried, when that
    directory does not exist — `resolve.rs`'s prefix list is built from what's actually on
    disk, not every theoretically possible layout.
  - The cross-file context walk (`ctx.rs`'s `Outline`, crossing into nested `group_mod!`s)
    is capped at depth 32 (`MAX_DEPTH`), a defensive limit against a pathological or cyclic
    module graph rather than an expected depth.
  - `group_mod!`'s declarations each expand independently, and errors accumulate across all
    of them in one invocation (`Error::combine`) rather than stopping at the first: a body
    with three declarations, two of them wrong, reports both failures in one compile.

## Step 4 — docs, metadata

- `docs/adr/004`: Status note — decisions 2–4 and the `mod create { run }` expansion
  superseded by ADR-005; 1 (otherwise), 5, 6 stand.
- `Cargo.toml` (both crates): `rust-version = "1.88"`; `[[example]] files`.
- `docs/PLAN.md` macro section, v1 scope, Verification (group discovery + file resolution
  in the golden matrix); `README.md` headline example → `#[verb]` + `#[group]`;
  `src/lib.rs` crate docs; `CLAUDE.md` line 5 ("one documented function"); `ROADMAP.md`;
  `.claude/skills/architect/SKILL.md` (core idea, crate layout, consumer shape, contracts).
- `docs/sketches/adr-004-build-plan.md` stays as history.

## Owner-confirmable calls made by the architect

1. `group_mod!` (forced by spike 0.2; alternatives `mod_group!`, `group_file!`).
2. `__verbandi_` prefix (cross-crate contract carries the crate name).
3. Context-free group is generic over `Ctx` (same rule as `#[verb]`).
4. `group_mod!` always emits `#[path]`.
5. `#[group(ctx = T)]` is written inside `group_mod!` on the declaration (so the body is the
   exact 1.99 item).
6. `commands!`: a bare path is always a handle; a local `Command` is `(c)`.
7. `App::command(create)` no longer compiles; use `app!`/`commands!` or the handle.
8. An external inside a group needs a hand-written `__verbandi_<name>` handle; the convention
   is documented public API.
9. Pushes, not `vec![]`, for cfg-able child registration.
10. `#[cfg]` on a verb/group is copied onto its own handle.
11. MSRV 1.88 recorded in `Cargo.toml`.
12. `group_mod!` top-level only, with a loud diagnostic.
13. `#[group]` takes no `name = "…"`.
