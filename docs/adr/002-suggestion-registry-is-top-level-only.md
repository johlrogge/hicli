# ADR-002: The suggestion gap-closer covers top-level names only

## Status

Accepted (2026-09-11)

**Unaffected in substance by [ADR-004](004-function-first-verbs.md) (2026-09-16).**
The scope rule — top-level command names only, applied to the command position
only — is unchanged. What changed is where the registry comes from: `App`
no longer accumulates names as `.leaf`/`.group` are called; `App::fold` derives
`Vec<&'static str>` from the top-level `Vec<Command<Ctx>>` at fold time
(`commands.iter().map(Command::name).collect()`), so there remains exactly one
list and no way for it to disagree with the tree — the property this ADR
already relied on, now harder to violate by construction.

Vocabulary renamed by [ADR-007](007-hicli-and-the-command-attribute.md) (`verbandi` → `hicli`, `#[verb]` → `#[command]`, `Verb` → `Invocation`, `VerbResult` → `Outcome`). The text below is kept exactly as written.

## Decision

`App::names` remains a flat list of **top-level** command names (leaves, mounted
groups and externals). `Group` neither collects nor exports its names, and there is
no registry of flags. The Levenshtein gap-closer is applied to the **command
position only** — the first argv word that is neither a flag nor a flag's value —
and only when bpaf produced no suggestion of its own.

## Why

`docs/PLAN.md` originally specified a Levenshtein pass over "registered names and
flags, nested included". Measurement contradicts the premise:

- bpaf suggests misspelled commands natively (since 0.5.1) and misspelled flags
  (since 0.7.10). `tests/cli.rs::typo_nested_command` and `typo_long_flag` pass
  through `App::parse`, which never reaches verbandi's `report_failure` — those
  assertions were always observing bpaf's own suggestions, not ours.
- A flat all-depths name set produces *misleading* suggestions: with `create` living
  only under `event`, a top-level `app creat` would be answered with
  ``did you mean `create`?`` — pointing at a command that does not exist at that
  position.
- Scanning every argv token (the original implementation) produces false positives
  on option values: `app status --home evnt` appended ``did you mean `event`?`` to
  an unrelated error, because `evnt` is the value of `--home`.

The gap-closer's job is therefore narrow and well-defined: the one position a
top-level registry can legitimately speak for is the top-level command word, in the
case where bpaf emits no hint at all.

Because verbandi does not know which globals take a value, a bare word directly
following a dashed token is assumed to be that option's value and is skipped. This
trades a false negative (`app -v stauts` yields no hint) for never emitting a false
positive — the strictly safer direction for an error message.

## Alternatives considered

- **Merge nested `Group` names into a flat `App::names` set** — cross-branch false
  positives (see Why); also duplicates suggestions bpaf already produces for nested
  commands.
- **Add a registry of global/leaf flag names** — duplicates bpaf ≥ 0.7.10 for no
  gain, and would need per-flag arity to be useful.
- **Position-aware nested registry (a tree, matched against the argv prefix)** —
  correct, but it reimplements bpaf's own command resolution inside verbandi for a
  case bpaf already handles. Mechanism without a consumer; revisit only with a
  concrete report of a missed suggestion.
- **Delete the gap-closer and the `strsim` dependency entirely** — tempting, and
  genuinely subtractive, but `docs/PLAN.md` lists "no typo-suggestion regression vs
  clap" as a hard requirement, and bpaf's suggestion threshold is not ours to
  control. Keeping a small, correct, tested safety net is cheap.

## Consequences

- `docs/PLAN.md` §"Typo / 'did you mean?' suggestions" point 2 must be corrected:
  the pass covers registered **top-level** names; nested commands and flags are
  covered by bpaf.
- `suggest` is rewritten to scan the command position only, stopping at `--`, at a
  recognised command name, and skipping option values (with `--opt=value` treated as
  self-contained). Ties are broken by registration order.
- The suggestion-appending step is extracted as a pure
  `augment_stderr(msg, names, args) -> String` so it can be unit-tested; previously
  it was reachable only through `App::run`, which no test calls, leaving verbandi's
  own gap-closer entirely uncovered.
- The hint is appended only in `App::run`. `App::parse` returns bpaf's
  `ParseFailure` untouched, so in-process tests observe bpaf's own suggestions,
  never verbandi's; the gap-closer's coverage is the unit tests in `src/app.rs`.
- If a consumer later reports a missed nested suggestion, the first move is a bpaf
  issue, not a deeper registry here.
