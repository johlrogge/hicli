# ADR-006 build plan (working document, not part of the crate)

Architect's plan for "an argument is a type", 2026-09-23. Read
`docs/adr/006-an-argument-is-a-type.md` and the args section of
`docs/sketches/moniker.rs` first.

## Step 0 — spike results (2026-09-23, scratchpad crate `spike3`, bpaf 0.9.28)

| # | Question | Answer |
|---|---|---|
| a | Completer on a `String` positional | Called for the empty word too. **bpaf does not prefix-filter completer output** (it does filter its own flag names). Rev-1 wire: 1 candidate → `value\n`; 0 or ≥2 → header `group\thelp\n` then `value[\tdesc]\n`; nothing applicable → `""`. |
| b | Completer on a `u64` positional | Empty word never reaches it (the trap). Called with the **parsed** value for `"1"`. |
| c | Completer under `some(..)` on `Vec<String>` | **Is called**, with the collected words including the in-progress one last; signature `fn(&Vec<String>)` (`clippy::ptr_arg`). |
| d | `construct!` elements | Only single-segment `ident()` calls; `<T as Tr>::parser()` and `a::b()` are rejected. Bound locals work; arity-1 `construct!(a0,)` is a pass-through `Parser<S>`. |
| e | `#[bpaf(external(a::b::f))]` | Multi-segment path accepted. |
| f | Trait + `#[diagnostic::on_unimplemented]` (two notes) | Compiles and renders on 1.97. `complete_with<T>` needs no extra bounds. `bpaf::ShellComp::Dir { mask: None }` compiles; emits `""` under rev 1. |
| g | `.complete(f)` on a multi-field derived struct | Compiles; `f` sees a fully built struct with fallbacks, cannot tell which field is being completed, never fires while another field is mid-word. Reject at type level. |
| — | **Hazard** | `run_inner(["--bpaf-complete-rev=2", ..])` **terminates the process**. Pin rev 1 in `App::complete`; never use rev 2 in tests. |

## Steps — ALL DONE 2026-09-23, `just ci` green (296 tests, 33 trybuild cases)

1. `src/arg.rs`: `Arg`, `Candidate`, `complete_with`/`complete_files`/`complete_dirs` (`pub mod arg`, no extra bounds needed)
2. `macros/src/arg.rs`: `#[derive(Arg)]` with `#[arg(complete = fn | files | dirs)]`; rejects union, generics, unknown key, `complete` on multi-field/enum, and a second `#[arg(...)]` list attribute; enum without completion accepted
6. `App::complete(args) -> Vec<String>` seam, rev 1 pinned; wire parsing in `app.rs::candidate_values`
3a. `#[verb]` emits a block binding `let aN = <T as ::verbandi::Arg>::parser();` (spanned at the parameter type) then `construct!(a0, a1,)`; `parse.rs` no longer snake_cases; `NOT_A_NAMED_TYPE` reworded
3b. All in-repo argument types derive `Arg` (never `Globals`); `tests/cli.rs::app_with_composed_args` keeps raw bpaf ctors; `src/lib.rs` doctests use a hand-written impl; `tests/verb_macro.rs` has a hand-written `Note` arg
4. trybuild: `verb_param_not_an_arg` (records the `on_unimplemented` text), `arg_two_attributes`, plus the five derive cases; `non_path_param`/`impl_trait_param` re-recorded
5. `tests/completion.rs` (5 tests, incl. the inheritance claim); both examples' `EventDir` completes dirs
7. Docs updated: `docs/PLAN.md`, `CLAUDE.md`, architect skill, `README.md`, `ROADMAP.md`, ADR-005 status note, `src/lib.rs`
8. ADR-006 written, Proposed

Known noise: a missing `#[derive(Bpaf)]` on a type named `Id` also gets rustc's unrelated
"consider importing `std::process::id`" hint; a wrong completer signature is reported twice.

## Owner-confirmable calls (defaults taken)

1. `Candidate` is a type alias, not a struct (breaking to change later).
2. `files`/`dirs` shorthands ship; masks deferred.
3. `#[arg(complete = …)]` on a multi-field struct is a compile error.
4. Composites hand-write `Arg::parser()`; field-composing derive deferred.
5. `App::complete` seam ships (pins rev 1; hides the rev-2 hazard).
6. Trait and derive both named `Arg`; helper attribute `#[arg(...)]`.
7. `Type::Path`-only rule for verb parameters stays.
