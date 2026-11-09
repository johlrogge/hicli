# ADR-014: A context that connects on first use needs no API

## Status

Accepted (2026-11-09)

Closes johlrogge/hicli#7 (`mk_ctx` seeing the selected command), taking the
third of the issue's suggestions: document a lazy context as the answer. No
code change: hicli adds nothing.

## Context

`App::run(mk_ctx)` parses, then calls `mk_ctx(globals) -> Result<Ctx, Failure>`,
then runs the command. mdma-cli's context resolves a node: a cache lookup,
mDNS discovery, a printed note for an implicit choice, a failure with advice
when nothing answers. Commands that never talk to a node (`node list`,
`generate-completions`) paid for that resolution, and failed when no node was
reachable. #7 asked for `mk_ctx` to see the selected command, so it could skip
the work for those, and listed `mk_ctx(G, &Invocation)`, a path-aware
`run_with`, or a documented lazy context as options.

mdma-cli has since solved it without hicli: its `Ctx::new` stores the globals
and a few cheap facts, and the node lives in a `OnceLock<Target>` resolved by
`Ctx::target()` the first time a command asks.

## Decision

hicli adds no API. A context with an expensive or fallible part builds that part
the first time a command asks for it:

```rust
pub struct Ctx {
    globals: Globals,
    target: OnceLock<Target>,
}

impl Ctx {
    pub fn new(globals: Globals) -> Self {
        Ctx { globals, target: OnceLock::new() }
    }

    /// The node to talk to, resolved on first use.
    pub fn target(&self) -> Result<&Target> {
        // `OnceLock::get_or_try_init` is unstable.
        if let Some(target) = self.target.get() {
            return Ok(target);
        }
        let resolved = resolve(self.globals.node.as_ref())?;
        Ok(self.target.get_or_init(|| resolved))
    }
}
```

1. **`mk_ctx` stays cheap.** It stores what was parsed and derives what
   costs nothing. A failure it can know without I/O — a bad combination of
   globals — is still a `Failure::usage` from `mk_ctx`.
2. **The expensive part is a `OnceLock<T>`** (or a `std::cell::OnceCell<T>`)
   behind a method returning `Result<&T, E>`: try `get`, else build, then
   `get_or_init`.
3. **A command asks with `?`.** ADR-012 turns the error into a runtime failure
   (exit 1), rendered as its chain. A resolution failure that is the user's to
   fix is mapped to `Failure::usage` where it is raised.
4. **Tests seed the cell** (`Ctx::with_target(globals, target)`), so a
   command's test needs no network, and a test can assert that a command never
   resolved (`ctx.target.get().is_none()`).

## Why

- **The need belongs to the command, and the command says it** where it calls
  `ctx.target()?`. Nothing restates which commands need a node.
- **A path-aware `mk_ctx` is a second dispatch.** A `match` on the selected
  command's path is the central, stringly typed table ADR-004 and ADR-005
  removed. It goes stale silently when a command is renamed, moved into a group,
  aliased, curried or made a default.
- **The standard library already has the tool.** `OnceLock` is the cell; a
  wrapper would be hicli owning a fallible-init type for no gain.

## Alternatives considered

- **`mk_ctx(globals, &Invocation)` or `run_with(|g, path| …)`** — hand
  `mk_ctx` the selected command. Rejected: the second dispatch above.
- **A per-command requirement** (`#[command(needs = Node)]`, or a context type
  per group). Rejected: an attribute dialect for what a method call says, and
  several context types multiply `App`'s type parameters.
- **A hicli `Lazy<T>`.** Rejected: `OnceLock` covers it; `LazyLock` cannot
  take the globals or return a `Result`.
- **Each command builds what it needs from the globals.** Rejected: commands
  do not receive the globals, and it repeats the building in every command.

## Consequences

- No API change. #7 closes with documentation: a README section, the `playr`
  example, and a line in `docs/PLAN.md`.
- A resolution error now comes from the command, after parsing and `mk_ctx`
  succeeded: exit 1 unless the command maps it to a usage error. `--help`,
  completion and parse errors never run `mk_ctx`, as before.
- Side effects of building — a printed note — happen at first use, in the
  command's output.
- `OnceLock` is `Sync`. With one command per process the "get, else build, then
  `get_or_init`" race cannot happen; if it ever did, the second value is
  dropped.
