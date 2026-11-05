# hicli — dev task runner. Mirrors what CI runs; keep in sync with
# CLAUDE.md's "Conventions" section.

default:
    @just --list

[group: 'dev']
build:
    cargo build --workspace --all-targets

[group: 'dev']
test:
    cargo test --workspace

[group: 'qa']
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

[group: 'qa']
fmt:
    cargo fmt --all

[group: 'qa']
fmt-check:
    cargo fmt --all -- --check

# The crate must build with its default `color` feature off (ADR-015).
[group: 'qa']
check-no-default:
    cargo check --workspace --all-targets --no-default-features

[group: 'qa']
ci: fmt-check clippy check-no-default test
    @echo "checks passed"

# Re-record the compile-fail snapshots (tests/ui/*.stderr) after an
# intentional change to a rejection message. Read the diff as the user who hits it.
[group: 'qa']
ui-bless:
    TRYBUILD=overwrite cargo test --test ui

# Run the example CLI — the living documentation. `just demo --help`,
# `just demo event create ./dir "My Event"`, …
[group: 'dev']
demo *args:
    cargo run --quiet --example demo -- {{ args }}

# The same shape with its groups in files (`#[group] pub mod x;`) rather than inline.
# `just demo-files --help`, `just demo-files event audience define ./dir Friends`, …
[group: 'dev']
demo-files *args:
    cargo run --quiet --example files -- {{ args }}

# Rehearse publishing both crates, in the order they must go out: the
# proc-macro crate first, because `hicli` exact-pins it.
#
# Why `--workspace` and not two `-p` runs: before `hicli-macros` exists on
# crates.io, `cargo publish --dry-run -p hicli` fails while *packaging* —
#
#     error: no matching package named `hicli-macros` found
#     location searched: crates.io index
#
# — and `--no-verify` does not help, because it only skips the later rebuild.
# `--workspace` (cargo 1.90+) packages both crates, orders them by their
# dependency, and verifies `hicli` against the locally packaged
# `hicli-macros`. It is also how the real publish runs: macros first.
#
# `flags` is for rehearsing mid-branch: `just publish-dry --allow-dirty`. A real
# pre-release run takes no flags, so an uncommitted file is still an error.
#
# cargo writes all of its progress to stderr; `2>&1` keeps a successful run
# visible to tools that only relay stdout.
[group: 'release']
publish-dry *flags:
    cargo publish --dry-run --workspace {{ flags }} 2>&1

[group: 'docs']
doc:
    cargo doc --no-deps

[group: 'maintenance']
clean:
    cargo clean
