# ADR-001: Every registered command carries a one-line help string

## Status

Accepted (2026-09-11)

## Decision

`App::leaf`, `App::group`, `Group::leaf` and `Group::group` take a required
`help: impl Into<String>` parameter between `name` and the parser/subgroup:

```rust
pub fn leaf(self, name: &'static str, help: impl Into<String>,
            p: impl Parser<BoxedVerb<Ctx>> + 'static) -> Self;
pub fn group(self, name: &'static str, help: impl Into<String>,
             sub: Group<Ctx>) -> Self;
```

The string is applied twice to the same command: as bpaf's `descr` (the heading of
`app <cmd> --help`) and as its `help` (the one-line entry in the parent's command
list). There is no `leaf_with_help` variant.

## Why

`App::leaf`/`Group::leaf` originally took `(name, parser)` only, so bpaf rendered
bare command names in help with no descriptions anywhere in the tree. Consumers
cannot fix this themselves: `leaf` takes `impl Parser<BoxedVerb<Ctx>>`, not
`OptionParser<_>`, so `.descr()` is not reachable from the call site — the
conversion to an `OptionParser` happens inside verbandi.

Making `help` required rather than optional:

- keeps one way to register a command, in line with the crate's subtractive scope;
- treats help text as non-optional, which is the correct default for a framework
  whose selling point is a usable command tree;
- makes `(name, help)` a uniform property of every node — `ExternalVerb` already
  carried `help` in its manifest, so leaf, group and external now agree;
- feeds one string to both surfaces (parent listing and command heading), so the
  two cannot drift.

## Alternatives considered

- **Add `leaf_with_help(name, help, p)` alongside `leaf(name, p)`** — two ways to
  register one thing, and the shorter one is the worse one. Rejected as additive.
- **Document that consumers set `.descr()` themselves** — impossible: `leaf` takes
  `impl Parser<_>`, not `OptionParser<_>`; the consumer never holds the value
  `.descr()` is defined on.
- **Derive help from the `Verb` type (e.g. a trait method or doc comment)** —
  would need a proc-macro or a new required trait method, both outside v1 scope.

## Consequences

- Breaking change against the in-repo 0.1.0. Acceptable: the crate is unpublished
  with zero external consumers. Migration is mechanical — add a string argument at
  every `.leaf`/`.group` call site (`examples/demo.rs`, `tests/cli.rs`, the
  `src/lib.rs` and `src/macros.rs` doc examples, and README snippets).
- Help output becomes assertable, so `tests/cli.rs` gains tests that the top-level
  help, group help and leaf help each carry their description.
- A future `--list-verbs` or `--dump-verb-manifest` handshake has the help text it
  needs already present on every node. This ADR does **not** build that registry:
  `App::names` stays `Vec<&'static str>` until a consumer needs more.
