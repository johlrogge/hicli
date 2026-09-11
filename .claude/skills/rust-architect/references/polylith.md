# Polylith Architecture in Rust

Polylith is a monorepo architecture for maximum component reuse across multiple deployable artifacts.
Originally from Clojure; this document describes the Rust/Cargo mapping.

## Component Granularity

> "Each brick does one thing, and if we want to do one more thing, then we create another brick."
> — Joakim Tengstrand

This is the most important thing to get right. Polylith components are **small by design**.

### Size guidelines

| Metric | Target |
|--------|--------|
| Lines of code | 100–1000, **~300 average** |
| Sub-modules | 1–4 |
| Public functions | As few as make sense for the concept |
| Dependencies on other components | Small — if a component depends on many others, it may be doing too much |

The poly tool itself (the reference Clojure implementation) has **41 components** across its codebase, averaging ~310 LOC each. Component names are intentionally granular: `config-reader`, `path-finder`, `ws-explorer`, `text-table`, `change` — not `configuration`, `file-paths`, `workspace-utilities`.

### When to split a component

Split when:
- A sub-module within the component could be independently useful elsewhere
- Two distinct concerns share a crate only because they appeared together first
- The component has grown past ~800 LOC and has clearly named sub-modules
- Another component could depend on just part of it

Do **not** split when:
- The concerns are truly inseparable (e.g. a parser and its AST types)
- Splitting would make the interface less clear without enabling any new reuse

### How to name components

Follow the "noun that does one thing" convention. The poly tool's components are instructive:

| Instead of… | Prefer… |
|-------------|---------|
| `template` (parsing + expressions + DOM + rendering) | `dom`, `expr`, `template-data`, `renderer` |
| `configuration` | `config-reader` |
| `file-paths` | `path-finder` |
| `workspace-utils` | `ws-explorer` |
| `utilities` | split into specific names |

A good component name is specific enough that a second component with a different name would clearly handle a different concern.

### Recognising over-large components

A component is probably too large if:
- It has internal sub-modules that have clear, independent names (`dom.rs`, `expr.rs`, `data.rs`)
- Its `lib.rs` re-exports from 4+ distinct sub-modules
- Another brick currently depends on ALL of it but only needs part of it
- Two different projects would want different subsets of its behaviour

## Core Concepts

### Component
A Cargo library crate under `components/<name>/`. Its **interface is the public surface of `lib.rs`** — the `pub` items re-exported from private submodules. Nothing beyond the crate boundary is accessible to other bricks.

```
components/user/
  Cargo.toml
  src/
    lib.rs        ← ONLY pub use re-exports
    user.rs       ← private implementation
```

```rust
// src/lib.rs — the interface: what other bricks can see
mod user;
pub use user::create_user;
pub use user::get_user;
pub use user::User;
```

```rust
// src/user.rs — the implementation: private
pub fn create_user(email: &str) -> Result<User> { ... }
pub fn get_user(id: UserId) -> Result<User> { ... }
```

**No traits required.** The interface is plain named functions, as Joakim Tengstrand (polylith inventor) intends. The crate's pub surface IS the interface contract.

### Package metadata (0.10.0+)
Workspace-wide package fields (`version`, `edition`, `authors`, `license`, `repository`) are declared in the root `Cargo.toml` `[package]` section. Prior to 0.10.0 these lived in `Polylith.toml [workspace.package]` — use `cargo polylith migrate-package-meta` or the `polylith_migrate_package_meta` MCP tool to move them.

### Interface metadata
Cargo-polylith uses an explicit metadata declaration in each component's `Cargo.toml`:

```toml
[package.metadata.polylith]
interface = "user"
```

New components get this automatically (`component new` defaults `interface` to the crate name).
Existing components can be updated with `component update <name> [--interface <NAME>]`.
`cargo polylith check` warns for any component missing this declaration.

### Base
A Cargo **library crate** under `bases/<name>/`. Exposes a runtime API (HTTP server, CLI, IPC, gRPC …) as ordinary Rust functions (`run()`, `serve()`, `create_sockets()`). Bases wire components together but do not hardcode which implementations are used.

**Bases must NOT have `src/main.rs`.** If a base were a binary, two bases could never share one process.

```
bases/http_api/
  Cargo.toml
  src/
    lib.rs        ← pub fn serve(...) / run(...) / create_sockets(...)
    handler.rs    ← private implementation
```

### Project
A plain **bin crate** under `projects/<name>/`. Owns `src/main.rs` and calls the bases' runtime-API functions. Projects CAN depend on components directly (valid polylith). Projects MUST depend on at least one base.

```
projects/production/
  Cargo.toml      ← [package] + [[bin]] only — NO [workspace] section
  src/main.rs     ← entry point: calls base fns, wires components
```

A project has **no domain logic** — all logic lives in bricks. The `main.rs` is a thin wiring point.

**Important (0.9.1+):** Projects must NOT have their own `[workspace]` section. They are members of the active profile workspace (root `Cargo.toml`). `ProjectHasOwnWorkspace` remains a hard error.

### Development Workspace
After `cargo polylith profile migrate`, the root `Cargo.toml` IS the active development workspace, generated from the dev profile. Run `cargo` directly for day-to-day development. Use `cargo polylith change-profile <name>` to switch the root workspace to a different profile, or `cargo polylith cargo --profile <name> <subcommand>` to temporarily build under a different profile without switching.

## Directory Layout

After `cargo polylith profile migrate`:

```
my-mono/
  Cargo.toml              ← root manifest; generated from active profile (dev by default)
  .cargo/config.toml      ← [build] target-dir = "target"  (shared across all workspaces)
  Polylith.toml           ← workspace version and versioning policy
  components/             ← library crates
    user/
      Cargo.toml
      src/lib.rs          ← pub re-exports only
      src/user.rs         ← private impl
    user_inmemory/        ← alternative implementation (same interface name "user")
      Cargo.toml
      src/lib.rs
      src/user.rs
  bases/                  ← runtime-API library crates (lib only, no main.rs)
    http_api/
      Cargo.toml
      src/lib.rs          ← pub fn serve(...)
  projects/
    production/
      Cargo.toml          ← [package] + [[bin]] only — NO [workspace] section
      src/main.rs         ← entry point: calls base fns
    test-env/
      Cargo.toml
      src/main.rs
  profiles/
    dev.profile           ← default dev implementation selections
    production.profile
```

## `cargo polylith check` Violations

**Hard errors** (non-zero exit, must fix):

| Violation | Meaning | Fix |
|-----------|---------|-----|
| `dep-key-mismatch` | Path dep key doesn't match `package.name` and no `package` alias | Add `package = "<real-crate-name>"` to the dep entry |
| `ProjectHasOwnWorkspace` | Project has its own `[workspace]` section — projects must be plain bin crates | Remove `[workspace]` from the project's `Cargo.toml` |

**Warnings** (exit 0, flag for attention):

| Violation | Meaning | Fix |
|-----------|---------|-----|
| `hardwired-dep` | Component/base has a direct `path = "..."` dep to another workspace component instead of `{ workspace = true }` | Move dep to `[workspace.dependencies]`, reference as `{ workspace = true }` |
| `profile-impl-not-found` | Profile references a component path that doesn't exist | Correct path in the `.profile` file or create the component |
| `profile-impl-not-component` | Profile references a path that is not a known workspace component | Add `[package.metadata.polylith]` metadata or point the profile at a proper component |
| `WildcardReExport` | `pub use foo::*` in lib.rs | Use named re-exports |
| `OrphanComponent` | Component not used by any project | Wire it or remove it |
| `ProjectFeatureDrift` | Project dep has fewer features than root workspace dep | Add missing features |
| `ProjectVersionDrift` | Project dep version differs from workspace | Align versions |
| `MissingInterface` | Component missing `[package.metadata.polylith] interface` | Add metadata |
| `not-workspace-version` | In relaxed versioning mode, brick's `Cargo.toml` does not use `version.workspace = true` | Set `version.workspace = true` in the brick's `Cargo.toml` |
| `AmbiguousInterface`, `DuplicateName`, `ProjectMissingBase`, `BaseHasMainRs` | Structural issues | Fix per violation name |

Notes:
- Projects depending directly on components is valid and not flagged.
- Bases depending on other bases is valid and not flagged.

## Swappable Implementations

Components share an interface name (via `[package.metadata.polylith] interface`) but can have different package names. Profiles select which implementation is active by mapping the interface name to a component path:

```toml
# profiles/dev.profile — development implementation selections
[implementations]
user = "components/user-stub"
storage = "components/storage-memory"

# profiles/production.profile — production implementation selections
[implementations]
user = "components/user"
storage = "components/storage-postgres"
```

All code calls `use user::UserService;` identically across profiles. The compiler enforces that both components expose the same public API — mismatched functions are compile errors. No traits needed.

### How Projects Select Implementations (0.11.0+)

After `profile migrate`, the root `Cargo.toml` IS the active workspace, generated from the dev profile. Bases declare component dependencies as **workspace-inherited deps** (`workspace = true`), resolved from the root `[workspace.dependencies]`. The active profile controls which implementations are wired in.

For day-to-day development, run `cargo` directly — the root workspace is the dev profile:

```bash
cargo check                                               # dev profile (active in root Cargo.toml)
cargo build
cargo test
```

To build under a different profile temporarily (restores root Cargo.toml after):

```bash
cargo polylith cargo --profile production build
cargo polylith cargo --profile production clippy -- -D warnings
```

To permanently switch the root workspace to a different profile:

```bash
cargo polylith change-profile production                  # overwrites root Cargo.toml
cargo build                                               # now builds with production implementations
cargo polylith change-profile dev                         # switch back
```

```toml
# bases/http_api/Cargo.toml
[dependencies]
user = { workspace = true }   # resolved from root [workspace.dependencies]
```

```toml
# Root Cargo.toml (generated from dev profile)
[workspace.dependencies]
user = { path = "components/user-stub" }   # from dev.profile
```

## Profiles

A **profile** is a named set of interface-to-implementation mappings stored as `profiles/<name>.profile`. Profiles let you switch the full set of component implementations with one command, rather than editing `Cargo.toml` files manually.

```toml
# profiles/production.profile
[implementations]
user = "components/user"
storage = "components/storage-postgres"
```

Profile commands:

```bash
cargo polylith profile migrate                            # one-time migration: generates root Cargo.toml from dev profile
cargo polylith profile new <name>                         # create a new empty profile
cargo polylith profile list [--json]                      # list defined profiles
cargo polylith profile add <interface> \
  --impl <path> --profile <name>                          # add/update one mapping
cargo polylith change-profile <name>                      # generate root Cargo.toml from named profile
cargo polylith check --profile <name>                     # validate as if profile were active
```

`profile migrate` reads root `[workspace.dependencies]` interface deps, writes `profiles/dev.profile` with those selections, regenerates the root `Cargo.toml` from the dev profile, and strips `{ workspace = true }` from brick `Cargo.toml`s so they are self-contained.

Since 0.14.0, a `.profile` file may also carry `[profile.*]` sections (e.g. `[profile.release]`). `change-profile` copies them into the generated root `Cargo.toml`, so per-profile compile settings (opt-level, lto, codegen-units, panic behaviour) travel with the profile.

## Project Cargo.toml Structure (0.9.1+)

Projects are plain bin crates — no `[workspace]` section. They live under `projects/` and are members of the profile workspace (not the root workspace after `profile migrate`).

```toml
# projects/production/Cargo.toml
[package]
name = "production"
version = "0.1.0"

[[bin]]
name = "production"
path = "src/main.rs"

[dependencies]
http_api = { path = "../../bases/http_api" }
cli      = { path = "../../bases/cli" }
# Bases' transitive component deps are resolved from the profile workspace [workspace.dependencies]
# Use `cargo polylith cargo --profile <name>` to switch implementations at build time
```

## Shared Target Directory

Without configuration, each profile workspace would have its own `target/` directory and recompile everything. Solve this with a single `.cargo/config.toml` at the repo root:

```toml
[build]
target-dir = "target"
```

Cargo hashes artifacts by (crate + features + profile + target triple), so identical builds across workspaces share compiled artifacts.

## The Development Workspace

After `profile migrate`, the root `Cargo.toml` IS the active development workspace, generated from the dev profile. It includes all components, bases, and projects as members:

```toml
# Root Cargo.toml (generated from dev profile)
[workspace]
members = [
  "components/*",
  "bases/*",
  "projects/*",
]

[workspace.dependencies]
# Implementations from dev.profile
user = { path = "components/user-stub" }
```

Run `cargo` directly for day-to-day development — LSP/rust-analyzer anchor to the root `Cargo.toml` naturally. Use `cargo polylith change-profile <name>` to switch profiles, or `cargo polylith cargo --profile <name> <subcommand>` to temporarily build under a different profile.

## What the cargo-polylith Tool Does

Managing this structure by hand is tedious. `cargo-polylith` handles:

- **Scaffolding**: `cargo polylith component new <name> [--interface <NAME>]` — always creates interface metadata (defaults to crate name)
- **Interface update**: `cargo polylith component update <name> [--interface <NAME>]` — set/replace interface on an existing component
- **Base scaffolding**: `cargo polylith base new <name>` creates `bases/<name>/` with `lib.rs` (pub fn run() skeleton) and Cargo.toml
- **Base update**: `cargo polylith base update <name> [--test-base]` — toggle test-base metadata on an existing base
- **Profile migration**: `cargo polylith profile migrate` — one-time setup; generates root Cargo.toml from dev profile
- **Profile switching**: `cargo polylith change-profile <name>` — generate root Cargo.toml from named profile
- **Build via profile**: `cargo polylith cargo [--profile <name>] <subcommand>` — temporarily swap root Cargo.toml with named profile, run cargo, restore original
- **Version bumping**: `cargo polylith bump [level]` — bump workspace version; level required in relaxed mode, auto-detected in strict mode
- **Implementation mapping**: `cargo polylith profile add <interface> --impl <path> --profile <name>` — add/update one interface-to-implementation mapping in a profile
- **Project management**: `cargo polylith project new <name>` generates the project bin crate manifest
- **Overview**: `cargo polylith deps` shows which components are used by which bases and projects
- **Interface checking**: `cargo polylith check` verifies structural correctness and reports violations
- **Interactive editor**: `cargo polylith edit` — TUI to toggle project/component connections, set interface names ('i' key), write all staged changes to disk ('w')

## Polylith in mdma

The `modular-digital-music-array` project at `~/projects/modular-digital-music-array` is the primary migration target. It has 27 components and 11 bases, plus a `projects/` layer with 9 projects.

Current status:
- Most bases are correctly lib crates: `http-server` exposes `serve()`, `service` exposes `create_sockets()`
- `mdma-library` base incorrectly has `main.rs` instead of `lib.rs` (flagged as warning by `cargo polylith check`)
- Three projects (`mdma-cli`, `mdma-gateway`, `mdma-tui`) have no base dependency yet (flagged as warnings)
- All components currently have no interface metadata — `cargo polylith check` will produce 27+ `missing-interface` warnings until `[package.metadata.polylith] interface` is added to each

`cargo polylith check` reports these violations to guide the migration.
