# ADR-006: An argument is a type — `Arg`, derivable, with completion

## Status

Proposed (2026-09-23)

Supersedes nothing. **Extends ADR-005's decision 1** (which restated ADR-004's):
the rule "each owned parameter's parser is the type's snake_case constructor"
moves out of `#[verb]` and into `#[derive(Arg)]`, where it becomes one line of
one impl instead of a call synthesised at every verb. ADR-005 §8 ("no
`#[arg(...)]` parameter attributes") **stands**: the `#[arg(...)]` introduced
here is a *derive helper attribute on a type*, written once beside the
`#[bpaf(...)]` that is already there — never on a verb's parameter. `#[verb]`
still never sees an argument's spelling.

Vocabulary renamed by [ADR-007](007-hicli-and-the-command-attribute.md) (`verbandi` → `hicli`, `#[verb]` → `#[command]`, `Verb` → `Invocation`, `VerbResult` → `Outcome`). The text below is kept exactly as written.

## Decision

### 1. `verbandi::Arg` — an argument is a type that parses itself

```rust
pub trait Arg: Sized + 'static {
    fn parser() -> impl bpaf::Parser<Self>;
}
```

One required method. An argument *is* how it parses, so completion is not a
second concept: a type that completes returns a parser that already carries its
completion. Return-position `impl Trait` in a trait is stable since Rust 1.75,
below the 1.88 MSRV.

`#[verb]` emits `<T as ::verbandi::Arg>::parser()` for each owned parameter,
bound to a local and composed with `construct!` in declaration order — the same
composition, the same trailing-comma rule at arity 1, the same `pure(())` at
arity 0. (Bound locals, not inlined calls: bpaf's `construct!` accepts only a
single-segment `ident()` call as an element — verified.)

### 2. verbandi ships no `Arg` impls

Not for `String`, not for `u64`, not for `PathBuf`. An impl for `PathBuf` has to
choose a metavar and positional-vs-option, and both choices would be silently
wrong most of the time — and would make `fn create(p: PathBuf, n: String, ctx:
&Ctx)` compile, which is the primitive-obsessed signature this ADR exists to
prevent. With no impls, that signature is a compile error carrying
`#[diagnostic::on_unimplemented]`:

```text
error[E0277]: `String` is not a verbandi argument type
  --> src/event.rs:9:26
   |
 9 | pub fn create(name: String, ctx: &Ctx) -> VerbResult {
   |                     ^^^^^^ this parameter's type must implement `Arg`
   |
   = note: an argument is a type: give it a newtype —
           `#[derive(Bpaf, Arg)] struct Name(#[bpaf(positional("NAME"))] String);`
   = note: verbandi ships no `Arg` impls for `String`, `u64`, `PathBuf`, … on
           purpose: the metavar and positional-vs-option are yours to name
```

The absence of the impls is the feature: it teaches the newtype pattern at the
moment and the place it is broken.

### 3. `#[derive(Arg)]`, beside `#[derive(Bpaf)]`

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Title(#[bpaf(positional("TITLE"))] pub String);
```

generates one impl calling the constructor `#[derive(Bpaf)]` wrote, named by
snake-casing the type's last path segment, **spanned at the type's ident** — so
a missing `#[derive(Bpaf)]` reports "cannot find function `title` in this scope"
pointing at `Title`, next to where the missing derive belongs.

The derive macro and the trait share the name `Arg` across the macro and type
namespaces and do not collide: one `use verbandi::Arg;` imports both, exactly as
`#[group]` and `verbandi::group` already do.

### 4. `#[arg(complete = …)]` — completion belongs to the type

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_ids)]        // fn(&Id) -> Vec<Candidate>
pub struct Id(#[bpaf(positional("ID"))] pub String);

#[arg(complete = dirs)]                // the shell's own directory completion
#[arg(complete = files)]               // the shell's own file completion
```

`pub type Candidate = (String, Option<String>);` — the value, and an optional
one-line description. A plain alias for what bpaf wants, so no conversion and no
turbofish. Three free functions carry the wiring and are public because the
expansion must be writable by hand: `complete_with`, `complete_files`,
`complete_dirs`.

Every verb taking an `Id` inherits `Id`'s completion, with nothing written at
the verb. That is the whole point.

`#[arg(complete = …)]` on a struct with more than one field, or on an enum, is a
compile error. Verified: a whole-struct completer is handed a fully built value
with fallbacks for untyped fields, cannot tell which field is being completed,
and never fires while another field is mid-word. The message names the two
right answers — make the value a newtype, or complete the field with bpaf's own
`#[bpaf(complete(f))]`.

### 5. Completion completes what already parses — and the completer filters

Verified against bpaf 0.9.28 in-process:

- bpaf hands a completion function the **parsed** partial value, `&T`. For a
  newtype over `String` it runs for every word including the empty one. For a
  newtype over `Vec<String>` (even under `some(..)`) it runs too, receiving the
  words collected so far with the in-progress word last, signature
  `fn(&Vec<String>)`. For a newtype over `u64`, or any `FromStr` that rejects
  the empty word, the **empty word never reaches the completer** — the first
  TAB yields nothing.
- bpaf passes a completer's output through **unfiltered** (while it does
  prefix-filter the flag names it derives itself). **The completer filters.**

The rule, stated rather than hidden: **`#[arg(complete = …)]` completes a value
that already parses, and returns only what matches the partial.** A type whose
inner value does not parse from a partial word hand-writes `Arg::parser()` and
completes at the string level before converting —

```rust
impl Arg for Port {
    fn parser() -> impl Parser<Self> {
        verbandi::complete_with(bpaf::positional::<String>("PORT"), known_ports)
            .parse(|s| s.parse::<u16>().map(Port))
    }
}
```

— which is why the trait is hand-implementable at all, and the same relationship
`#[verb]` has to `verbandi::command`.

### 6. A completion function reads the environment, not the globals

bpaf hands a completion function only the partial value; it cannot see `--home`.
The v1 rule is that anything else it needs comes from the environment
(`$MONIKER_HOME`), and that it is fast, never panics, and answers an unreadable
world with an empty `Vec` — it runs in the user's shell on every TAB.
"Pre-parse the globals leniently in completion mode" is deferred: it needs a
type-erased process-global stash with a downcast that can fail at runtime, and a
second all-optional globals parser that verbandi cannot synthesise.

### 7. A composite composes `Arg`s

A composite that wants its components' completion writes `Arg::parser()` by
hand, composing `<Part as Arg>::parser()` with `construct!` — six lines.
`#[bpaf(external(part))]` cannot do it, because it names bpaf's derived
constructor, which has no completion. A composite whose parts complete nothing
just derives `Bpaf, Arg` and loses nothing. A derive that composes fields
automatically is deferred.

### 8. `App::complete(args) -> Vec<String>` — the completion test seam

In-process, no shell: appends bpaf's completion request at protocol revision 1
and returns the candidate values. It exists so consumers' tests never touch the
protocol revision. Verified hazard: **revision 2 driven through `run_inner`
terminates the process** (a hole in bpaf's exit-free seam, outside verbandi's
control), so the seam pins revision 1 and documents why. Revision 1 cannot
express shell directives, so `complete = dirs` produces no values there — tests
assert it compiles and parses.

## Why

- **The newtype becomes load-bearing instead of conventional.** Before, a
  newtype argument was a style the docs recommended; the compiler was equally
  happy with `fn create(name: String, …)` as long as a free `fn string()`
  existed somewhere. Now the type *is* the interface: it carries the metavar,
  the help, the validation and the completion, and a primitive in a verb's
  signature is a compile error that explains itself.
- **Completion was unreachable where it mattered.** bpaf's `complete` is
  per-parser, and a verb's parsers were synthesised inside `#[verb]`'s expansion
  — a place no consumer can reach. Moving the parser into the type puts
  completion exactly where a consumer already writes everything else about an
  argument, and makes it inherit: declare it once on `Id`, get it in every verb
  that takes an `Id`, including ones not written yet.
- **A convention resolved at the call site became a trait bound.** "Call the
  snake_case constructor" was checked once per *parameter*, in generated code,
  producing "cannot find function `id`" at a hidden handle. It is now checked
  once per *type*, at the type, and the verb-side failure is an
  unimplemented-trait error with notes we write.
- **`#[verb]` got smaller.** `parse.rs` stops deriving a name from a type and
  `expand.rs` stops synthesising a call; `shared::snake_case` moves rather than
  duplicating.
- **The escape hatch is the design.** Everything the derive cannot express —
  a non-`String` inner type, a validated newtype, a composite — is a hand-written
  `Arg::parser()`, which is ordinary Rust over bpaf's public API.

## Alternatives considered

- **`fn complete(partial: &str) -> Vec<Candidate>` as a defaulted trait
  method.** Rejected: bpaf never hands a completion function a `&str`, only the
  parsed `&T`. Honouring the `&str` shape would require verbandi to build the
  parser string-first, which means owning argument spelling — the thing ADR-004
  and ADR-005 both refused.
- **A blanket `impl<T: Bpaf> Arg for T`.** Impossible: `#[derive(Bpaf)]` emits a
  free function, not a trait impl, so there is nothing to key on. And unwanted
  even if it existed — coherence would then forbid the hand-written impls that
  are the escape hatch.
- **`Candidate` as a struct.** Rejected for v1: the alias pins `M = String`,
  which removes the only real papercut, at no cost. Revisit only if consumers
  find the tuple opaque — it is a breaking change, so it is decided now.
- **Shipping `impl Arg for PathBuf` (and friends).** Rejected: see decision 2.
- **`#[derive(Arg)]` composing a struct's fields from their own `Arg`s.**
  Deferred, not rejected.
- **`#[arg(parser = some_fn)]`** for a type whose bpaf constructor was renamed.
  Deferred: a three-line hand-written impl covers it.
- **Masks on the shell shorthands (`complete = files("*.toml")`).** Deferred.
- **Filtering completer output inside `complete_with`.** Not possible
  generically — `complete_with` sees `&T`, not its string form. The derive could
  filter for `String` newtypes only; rejected as a special case that would
  differ from hand-written impls. The rule "the completer filters" is uniform.

## Consequences

- **Breaking against the in-repo 0.1.0**, still unpublished. Every argument type
  that is a verb parameter gains `Arg` in its derive list; the hand-spelled
  `Title` in `src/lib.rs`'s doctests gains a hand-written impl.
- **`Globals` does not implement `Arg`.** `App::new(globals())` takes a parser
  directly; globals are not a verb parameter.
- **Hand-written `verbandi::command(name, help, parser, run)` is unchanged.** It
  takes any `impl Parser<A>` and never mentions `Arg`.
- **bpaf's `autocomplete` feature is now load-bearing**; it is already enabled.
- **Completion is tested in-process** through `App::complete`, with no shell.
- **The `#[verb]` golden expansion changes**: parsers are bound to locals
  (`let a0 = <EventRef as ::verbandi::Arg>::parser();`) before `construct!`.
- **Positional ordering is unchanged**: adding completion never changes a
  type's positional-ness, so nothing moves in a verb's parameter list.
