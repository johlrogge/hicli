# verbandi — dev task runner. Mirrors what CI runs; keep in sync with
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

[group: 'qa']
ci: fmt-check clippy test
    @echo "checks passed"

[group: 'docs']
doc:
    cargo doc --no-deps

[group: 'maintenance']
clean:
    cargo clean
