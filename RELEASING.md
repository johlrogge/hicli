# Feature & Release Workflow

verbandi uses git flow with a multi-agent workflow for both feature development and
releases (inherited from metadev — see metadev's own `RELEASING.md` for the source of
this template).

## Branch Model

- `main` — released code only, always tagged
- `develop` — integration branch, features merge here
- `feature/*` — individual features, branch from develop
- `release/*` — release prep, branch from develop
- `hotfix/*` — urgent fixes, branch from master

## Versioning (semver)

`verbandi` is a crates.io library — semver compatibility is a hard external contract,
not just a convention:

- `feat` commits → minor bump (0.4.0 → 0.5.0)
- `fix` / `chore` commits → patch bump (0.5.0 → 0.5.1)
- Breaking change (`!`) → major bump (0.5.0 → 1.0.0)

Before `1.0.0`, minor bumps (`0.x.0`) may still carry breaking changes per Cargo's
pre-1.0 semver convention — but mark them `!` regardless, so the commit history stays
honest.

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
   - `cargo publish --dry-run` succeeds
   - `Cargo.toml` metadata (`description`, `license`, `repository`, `keywords`,
     `categories`) is complete
   - `examples/` CLI runs and demonstrates the current API

5. **release-manager** — start and finish the release branch
   > "Start release 0.x.0" → confirm → "Finish release 0.x.0"

6. **Human** — push to remote and publish
   ```
   git push origin main develop --tags
   cargo publish
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
- `verbandi` has no Polylith workspace / profile concerns — it's a single crate
