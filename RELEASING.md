# Feature & Release Workflow

hicli uses git flow with a multi-agent workflow for both feature development and
releases (inherited from metadev — see metadev's own `RELEASING.md` for the source of
this template).

## Branch Model

- `master` — released code only, always tagged
- `develop` — integration branch, features merge here
- `feature/*` — individual features, branch from develop
- `release/*` — release prep, branch from develop
- `hotfix/*` — urgent fixes, branch from master

## Versioning (semver)

`hicli` is a crates.io library — semver compatibility is a hard external contract,
not just a convention:

- `feat` commits → minor bump (0.4.0 → 0.5.0)
- `fix` / `chore` commits → patch bump (0.5.0 → 0.5.1)
- Breaking change (`!`) → major bump (0.5.0 → 1.0.0)

Before `1.0.0`, minor bumps (`0.x.0`) may still carry breaking changes per Cargo's
pre-1.0 semver convention — but mark them `!` regardless, so the commit history stays
honest.

## Two crates, one version

The workspace publishes **two** crates, and they move as one:

- `hicli-macros` — the proc macros `#[command]`, `#[group]`,
  `commands!` and `#[derive(Arg)]`, re-exported as `hicli::command`,
  `hicli::group`, `hicli::commands` and `hicli::Arg`
- `hicli` — the runtime crate, which depends on it as
  `hicli-macros = { version = "=0.1.0", path = "macros" }`

Rules:

1. **The pin is exact (`=`).** `#[command]` expands into hicli's expansion shape,
   which is private by contract, so a mismatched pair is a broken pair.
2. **Bump both `Cargo.toml`s in the same commit** — `macros/Cargo.toml`'s
   `[package] version` and the root `Cargo.toml`'s `hicli-macros = { version = "=X.Y.Z" }`
   — so the two versions are never observed apart.
3. **Publish `hicli-macros` first.** `hicli` cannot be packaged until its exact
   pin resolves against the crates.io index. `just publish-dry` runs
   `cargo publish --dry-run --workspace`, which orders the two correctly and verifies
   `hicli` against the *locally* packaged macro crate; the real publish is
   `cargo publish --workspace` (or the two `-p` runs by hand, macros first, waiting for
   the index in between).
4. **Name-check both crates before the first release.** `hicli` *and*
   `hicli-macros` must be free on crates.io — a taken macro-crate name blocks the
   release just as hard as a taken runtime-crate name. Check both at
   <https://crates.io/crates/hicli> and <https://crates.io/crates/hicli-macros>
   (ADR-007 renamed both),
   and confirm neither is a squatted placeholder.

## MSRV

`rust-version = "1.99"`, set in **both** `Cargo.toml` and `macros/Cargo.toml`.

1.99 is where attribute macros on file-module declarations stopped being
feature-gated, which is what `#[group] pub mod x;` needs (ADR-010).
`proc_macro::Span::local_file` (stable since 1.88) is still how `#[group]` finds the
declaring file. Older toolchains are not supported, so the number is a real floor and
not a hope.

Rules:

1. **Both manifests carry it, and they are bumped together**, in the same commit, for
   the same reason the versions are: the two crates are one artefact, and a macro crate
   that needs a newer compiler than its runtime crate admits to is a broken pair.
2. **Raising it is a breaking change for semver purposes** — mark the commit `!` and
   say which feature bought the bump.
3. **Say it once more in `README.md`** ("Rules and papercuts"), which is the only place
   a consumer reads before adding the dependency.

## Feature Workflow

New features go through a design-implement-review loop before release.

1. **product-owner** — prioritize and scope the feature on the roadmap
   > "Should we add X? Where does it fit in the roadmap?"

2. **release-manager** — start the feature branch
   > "Start feature <name>"

3. **architect** — design the feature; produce a task breakdown for minions
   > "Design the implementation for <feature>"
   The architect produces clear, scoped tasks — one per minion. Architectural
   decisions must stay consistent with `docs/PLAN.md`; if a feature requires
   deviating from `docs/PLAN.md`, that document gets updated first.

4. **code-minions** — implement in parallel, one task each
   > Spawn multiple minions simultaneously with different task descriptions.
   Each minion: writes a failing test → implements → runs checks → reports back.

5. **architect** — review all minion output
   > "Review the changes from <minion tasks>"
   - If approved: says COMMIT with a suggested message
   - If changes needed: dispatches minions again with specific fix instructions
   - Loop until approved

6. **commit** — commit the approved changes
   > Invoked by the orchestrator when architect approves

7. **release-manager** — finish the feature branch
   > "Finish feature <name>"

## Release Checklist

Run these agents in order before cutting a release:

1. **architect** — review all changes since last release

2. **product-owner** — confirm the release delivers intended value

3. **documenter** — update README.md / docs/PLAN.md to reflect the release

4. Verify crates.io readiness:
   - `just publish-dry` succeeds — both crates, in order (see "Two crates, one
     version")
   - `Cargo.toml` metadata (`description`, `license`, `repository`, `readme`,
     `keywords`, `categories`) is complete **in both** `Cargo.toml` and
     `macros/Cargo.toml`
   - the exact pin in the root `Cargo.toml` matches `macros/Cargo.toml`'s version
   - `rust-version` is the same in both manifests and matches what `README.md` says
   - both names are still free on crates.io (first release only)
   - `examples/` CLI runs and demonstrates the current API (`just demo --help`)

5. **release-manager** — start and finish the release branch
   > "Start release 0.x.0" → confirm → "Finish release 0.x.0"

6. **Human** — push to remote and publish
   ```
   git push origin master develop --tags
   cargo publish --workspace      # hicli-macros first, then hicli
   ```

## Hotfix Checklist

1. **release-manager** — start hotfix
2. **commit** — commit the fix
3. **release-manager** — finish hotfix (confirm before calling)
4. **Human** — push and, if the hotfix affects a published version, publish a patch
   release to crates.io

## Notes

- Agents never push or publish to crates.io — that always stays with the human
- Always confirm with release-manager before finishing a release or hotfix
- Multiple code-minions can run in parallel on different tasks within the same feature
- The commit agent invokes the conventional-commits skill for commit message format
- The architect never writes code — it designs and reviews only
- `hicli` has no Polylith workspace / profile concerns — it is a plain two-member
  cargo workspace (`.` and `macros`), published as two crates in one step
