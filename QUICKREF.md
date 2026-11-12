# hicli quick reference

One page for an agent (or a busy human) who knows clap. Every `rust` block is a
doctest, so it compiles as written; `# ` lines are hidden boilerplate. The long
version is `README.md`; the design is in `docs/adr/`.

## 1. App skeleton

```toml
[dependencies]
hicli = "0.7"   # that is all: no bpaf line needed
```

```rust,no_run
use hicli::{app, command, App, Arg, Failure};
use std::process::ExitCode;

/// The node to talk to
#[derive(Debug, Clone, Arg)]
#[arg(long = "node", metavar = "HOST", env = "PLAYR_NODE")]
pub struct Globals(pub Option<String>);

pub struct Ctx { node: String }

/// Check that the node answers
#[command]
fn ping(ctx: &Ctx) {
    println!("pong from {}", ctx.node);
}

// Runs once, after parsing, before any command. Keep it cheap.
fn mk_ctx(g: Globals) -> Result<Ctx, Failure> {
    Ok(Ctx { node: g.0.unwrap_or_else(|| "localhost".into()) })
}

fn app() -> App<Globals, Ctx> {
    app! { globals: globals(), commands: [ping] }
        .about("playr: control the music player")
        .version(env!("CARGO_PKG_VERSION"))
}

fn main() -> ExitCode {
    app().run(mk_ctx) // returns the code; never calls process::exit
}
```

- `commands: [..]` takes bare names: a command fn or a `#[group]` module.
- Exit codes: `0` ok and `--help`, `1` runtime failure, `2` usage error.
- No globals: `globals: hicli::bpaf::pure(())`, and `App<(), Ctx>`.
- `run` prints the binary's own name in usage lines; tests use `.bin_name("playr")`.
- clap: `#[derive(Parser)]` + `enum Commands` + `match` become `app!`.

## 2. A command is a documented function

The first doc paragraph is the help, the rest is details, and `# Examples` is a footer.
A doc comment is required. `set_output` is `set-output`, or `#[command(name = "..")]`.
The fn stays plain Rust and stays callable. `&Ctx` goes last, or is omitted.

````rust
# use hicli::command;
# struct Ctx;
/// Remove stale files from the library
///
/// Deletes nothing outside the library directory.
///
/// # Examples
///
/// ```sh
/// playr prune
/// ```
#[command(alias = "gc")]
fn prune(ctx: &Ctx) -> std::io::Result<()> {
    std::fs::read_dir(".")?; // `?` works: any error becomes a failure, exit 1
    Ok(())
}
# fn main() { prune(&Ctx).unwrap(); } // still just a function
````

`--help` ends with the footer, after the options:

```text
Examples:
  playr prune
```

- Return `()`, `Outcome`, `io::Result<()>`, `anyhow`/`color_eyre::Result<()>` or
  `Result<(), Box<dyn Error>>`. The message is the chain, `outer: mid: root`.
- A usage error from a command: `Err(Failure::usage("..."))`. Exit code n: `Ok(Success::code(n))`.
- clap: variant + `about` + `match` arm become one fn.

## 3. Groups, aliases, default, currying

A group is a module; its children are the marked items in it, in source order.

```rust
# use hicli::{app, command, group, App, Arg};
# use hicli::bpaf::pure;
# struct Ctx;
# /// A source on the node
# #[derive(Debug, Clone, Arg)]
# #[arg(positional = "SOURCE")]
# pub struct SourceName(pub String);
/// Playback control
#[group(alias = "pb")]
mod playback {
    use super::*;

    /// Stop playback
    #[command(alias = "halt")]
    pub fn stop(ctx: &Ctx) {}

    /// Dump internals (parses, but is not in help or completion)
    #[command(hide)]
    pub fn debug(ctx: &Ctx) {}

    /// Show the player's state: bare `playback` runs this
    #[command(default)]
    pub fn status(ctx: &Ctx) {}
}

/// Music sources: `source bandcamp check-updates`
#[group(args = SourceName)]
mod source {
    use super::*;

    /// List the sources (a fixed sibling: takes no source name)
    #[command]
    pub fn list(ctx: &Ctx) {}

    /// Check a source for stale items (receives the group's `SourceName`)
    #[command(curried)]
    pub fn check_updates(s: SourceName, ctx: &Ctx) {}
}

fn main() {
    let _app: App<(), Ctx> = app! { globals: pure(()), commands: [playback, source] };
}
```

- A group in its own file: `/// doc` + `#[group] pub mod library;`, at the top level of
  a file (never inside an inline module). The same keys apply.
- `#[group(curried = T)]` is a group nested in a curried slot: every child is curried
  over the same `T`. The curried value is the command's first owned parameter.
- A default: at most one per group, never top level, no alias or `hide`, positionals
  last. With a positional, `search jazz` runs the default with `jazz`; a named sibling
  wins its own word.
- `source bandcamp` alone: ``expected a command after `bandcamp`: `check-updates` ``.
- clap: nested `enum Subcommand` becomes `#[group] mod`; `visible_alias` becomes `alias`.

## 4. An argument is a type

A parameter is a type that parses itself and names itself in help and errors.
`fn create(name: String)` is a compile error by design:

```text
/// create a thing
#[command]
fn create(name: String, ctx: &Ctx) {}   // error[E0277]: `String` is not a hicli argument type
```

Spell it with `#[derive(Arg)]` and `#[arg(..)]` on the type, never on a field. The
type's doc comment's first paragraph is its help row in every command that takes it.

| key | meaning |
|---|---|
| `positional = "NAME"` | a positional. Bare `positional` is the ident: `SortBy` is `SORT-BY` |
| `long = "name"`, `short = 'c'` | an option, alone or together. Bare `long` is `--sort-by` |
| `metavar = "N"` | the option's value name. Required, except for a `bool` switch |
| `env = "VAR"` | environment fallback, shown in help. Options with a value only |
| `complete = f` / `files` / `dirs` | completion (section 9) |

What is inside decides the rest: `T(Option<V>)` is optional, `T(Vec<V>)` repeats
(zero or more), `T(bool)` with a name is a switch, anything else is one word.

```rust
use hicli::{command, Arg, bpaf::Parser};
use std::{num::NonZeroUsize, path::PathBuf, str::FromStr};

/// How many to take
#[derive(Debug, Clone, Copy, Arg)]
#[arg(long = "limit", short = 'l', metavar = "N")]
pub struct Limit(pub NonZeroUsize);

// The type's own FromStr wins over NonZeroUsize's. Its error is a phrase that
// reads after the argument's name.
impl FromStr for Limit {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Limit).map_err(|_| "must be at least 1")
    }
}

/// Only tracks added in the last DAYS days, e.g. -7
#[derive(Debug, Clone, Arg)]
#[arg(long = "added", metavar = "DAYS")]
pub struct Added(pub Option<NonZeroUsize>);

/// Audio files to import: a plain `Vec<PathBuf>`, as one type
#[derive(Debug, Clone, Arg)]
#[arg(positional = "FILE", complete = files)]
pub struct Inputs(pub Vec<PathBuf>);

/// List tracks
#[command]
fn list(limit: Option<Limit>, added: Added, files: Inputs) {}

fn main() {
    let e = <Limit as Arg>::parser().to_options().run_inner(&["-l", "0"][..]).unwrap_err();
    assert_eq!(e.unwrap_stderr(), "couldn't parse `0`: `--limit`: must be at least 1");
    let i = <Inputs as Arg>::parser().to_options().run_inner(&["a.flac", "b.flac"][..]).unwrap();
    assert_eq!(i.0.len(), 2);
}
```

- Which `FromStr` parses a word: `T(V)` uses `T`'s own if it has one, else `V`'s;
  `T(Option<V>)` and `T(Vec<V>)` use `V`'s. A struct with several fields is one word and
  needs its own `FromStr`.
- Errors name the spelling. From the environment:
  ``couldn't parse: `--node` from `$PLAYR_NODE`: ..``, never echoing the value. `--help`
  prints a set variable's value, so no secrets in `env` options.
- Values that start with `-` are written glued: `--added=-7`. `--added -7` is an error.
- clap: `value_parser` becomes `FromStr`; `#[arg(env)]` becomes `env`.

## 5. `Option<T>`: "T, if given"

In a command's parameters or a composed struct's fields, `Option<T>` is `T`'s parser
made optional; `T` keeps its own `FromStr`, help and completion. In section 4,
`limit: Option<Limit>` is that, and its usage reads `list [--limit=N]`.

- `Option<T>` at the use is the default form: `T` stays required wherever it is not wrapped.
- Optional at every use (a filter, a query, a global)? Hold the `Option` inside, as
  `Added(Option<V>)` does; `V`'s `FromStr` parses.
- `Option<T>` of an already-optional `T` is a compile error (E0080, "`..` succeeds when
  its argument is absent"): `Option<Added>`, `Option<Option<T>>`, an `Option` of a switch,
  a `Vec` or an all-optional struct. An `Option` of a composite with required fields is
  all-or-nothing: one word of two is an error, not `None`.
- clap: `Option<T>` fields. `default_value` has no key: default in the body or `mk_ctx`.

## 6. `Choice`: a fixed set of words

`#[derive(Choice)]` makes a fieldless enum a set of kebab-case words (`PlayCount` is
`play-count`; `#[choice(name = "..")]` on a variant renames). With `#[derive(Arg)]` it is
itself an argument: it parses, completes (each variant's doc is its description) and
ends its help row with the names, so do not list them in the doc comment.

```rust
use hicli::{Arg, Choice};

/// What to sort by
#[derive(Debug, Clone, Copy, PartialEq, Choice, Arg)]
#[arg(positional = "KEY")] // or: #[arg(long = "by", metavar = "KEY")]
pub enum SortKey {
    /// Beats per minute
    Bpm,
    /// Track title
    Title,
    /// Artist name
    Artist,
}

fn main() {
    let e = "bmp".parse::<SortKey>().unwrap_err();
    assert_eq!(e.to_string(), "expected `bpm`, `title` or `artist`; did you mean `bpm`?");
}
```

```text
$ playr sort bmp
Error: couldn't parse `bmp`: `KEY`: expected `bpm`, `title` or `artist`; did you mean `bpm`?
```

clap: `#[derive(ValueEnum)]`.

## 7. Composed structs and globals

A struct with named fields, no `#[arg(..)]` keys and no `#[bpaf(..)]` is composed from
its fields' types: each is an argument type, or `Option` of one. A field's doc comment
replaces its type's help row. The struct's doc comment is a heading above the rows.
Positional fields go last. Globals are the same: one is a spelled newtype (section 1),
several are a composed struct. They are accepted before or after the subcommand.

```rust
# use hicli::{app, command, App, Arg};
/// The node to talk to
#[derive(Debug, Clone, Arg)]
#[arg(long = "node", metavar = "HOST", env = "PLAYR_NODE")]
pub struct Node(pub Option<String>);

/// Print more
#[derive(Debug, Clone, Arg)]
#[arg(short = 'v', long = "verbose")]
pub struct Verbose(pub bool);

/// Global options
#[derive(Debug, Clone, Arg)]
pub struct Globals {
    /// Defaults to localhost
    pub node: Node,
    pub verbose: Verbose,
}

# pub struct Ctx;
# /// Check that the node answers
# #[command]
# fn ping(ctx: &Ctx) {}
fn main() {
    let _app: App<Globals, Ctx> = app! { globals: globals(), commands: [ping] };
}
```

A command takes a composed struct as one parameter (`fn export(a: ExportArgs)`). A
fallback is an `Option` defaulted in `mk_ctx`. clap: `#[command(flatten)]` is a composed
struct; `global = true` is `globals:`.

## 8. Per-command help override: `# Arguments`

A type's help is written once. One command can say something more specific in an
`# Arguments` section: `` * `param` - help `` (a `-` bullet works; an entry may continue
on indented lines). It is not printed as details, and only that help row changes.

```rust
# use hicli::{command, Arg, Failure, Outcome};
# struct Ctx;
# /// Do not ask for confirmation
# #[derive(Debug, Clone, Arg)]
# #[arg(short = 'y', long = "yes")]
# pub struct Yes(pub bool);
/// Permanently delete everything in the archive
///
/// # Arguments
///
/// * `yes` - Confirm: the files cannot be recovered
#[command]
pub fn empty(yes: Yes, ctx: &Ctx) -> Outcome {
    if !yes.0 {
        return Err(Failure::usage("this cannot be undone: pass --yes to confirm"));
    }
    Ok(())
}
# fn main() {}
```

An entry is a compile error for a `#[derive(Bpaf)]` type, a composed struct, the `&Ctx`, a curried
value, an unknown name, a group or a `#[command(default)]`.

## 9. Completion

Completion belongs to the type and is inherited by every command that takes it. `files`
and `dirs` hand paths to the shell. `complete = f` takes `fn(&str) -> Vec<Candidate>`,
is given the raw word on every keystroke, and **must filter by prefix itself**. A
`Choice` needs no key. Command names complete by themselves.

```rust
# use hicli::{command, Arg, Candidate};
/// A source on the node
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SOURCE", complete = complete_sources)]
pub struct SourceName(pub String);

fn complete_sources(typed: &str) -> Vec<Candidate> {
    let all = ["bandcamp", "beatport", "upload"];
    all.into_iter().filter(|s| s.starts_with(typed)).map(|s| (s.to_string(), None)).collect()
}

/// A shell to print completions for
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SHELL")]
pub struct ShellArg(pub hicli::Shell); // a Choice: bash, zsh, fish

/// Print the shell completion script
#[command(hide)]
fn generate_completions(shell: ShellArg) {
    print!("{}", hicli::completion_script(shell.0, "playr"));
}
# fn main() {}
```

Install with `playr generate-completions zsh > ~/.zfunc/_playr`. clap: `clap_complete`.

## 10. Testing, in-process

No subprocess, no golden files. `App::parse` returns `(globals, Invocation)` or the error
that would be printed (`unwrap_stderr()`; `unwrap_stdout()` for `--help`).
`exit_code_for(&e)` is 2 for usage and 0 for help. `cli!` turns typed words into an argv
(whitespace separates words; `"My Event"` is one). `cli_test!` writes one `#[test]` a line.

```rust
# use hicli::{app, cli, cli_test, command, exit_code_for, App, Arg};
# use hicli::bpaf::pure;
# struct Ctx;
# /// An audio output device
# #[derive(Debug, Clone, Arg)]
# #[arg(positional = "NAME")]
# pub struct Output(pub String);
# /// Select an audio output device
# #[command]
# fn set_output(output: Output, ctx: &Ctx) {}
fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [set_output] }
}

cli_test! {
    app: app();
    needs_a_name: { set-output } => |r| {
        let e = r.err().expect("a usage error");
        assert_eq!(exit_code_for(&e), 2);
        assert_eq!(e.unwrap_stderr(), "expected `NAME`, pass `--help` for usage information");
    },
}

# fn main() {
// By hand; `.bin_name("playr")` makes usage lines read as `run` prints them.
let (_globals, invocation) = app().bin_name("playr").parse(cli! { set-output hdmi }).unwrap();
assert_eq!(invocation.path(), ["set-output"]);
# }
```

- `app().run_with_args(cli! { .. }, mk_ctx)` runs everything and returns the exit code;
  `app().complete(cli! { sort "" })` returns what the shell would offer;
  `invocation.run(&ctx)` runs the selected command against a context you built.

## 11. Gotchas

1. **Constructor name clash.** `#[derive(Arg)]` writes a snake_case fn beside the type
   (`Filters` writes `filters()`). A `#[command] fn filters` in the same module is E0428,
   "the name `filters` is defined multiple times". Name types for what they carry
   (`FilterArgs`), not for a command.
2. **Positionals last**, in a command's parameters and a struct's fields. A debug build
   panics the first time the app runs; a release build when `--help` renders. Flags can
   still go anywhere on the command line.
3. **`missing_docs` on composed structs.** A struct doc is a heading and a field doc an
   override, so an undocumented one is deliberate: `#[allow(missing_docs)]` on them. A
   command, group or spelled type with no doc comment is a compile error.
4. **`#[arg(..)]` goes on a type, never on a field or a parameter.** Parameters have no
   attributes. Spell the value on a newtype.
5. **`Option` of an already-optional type** is a compile error (section 5).
6. **When you still need `#[derive(Bpaf)]`:** a guard, `some` (one or more) or a fallback
   shown in help. Write `#[derive(Bpaf, Arg)]` with a `#[bpaf(..)]` attribute (without
   one the struct is composed, and E0428) and add `bpaf = "0.9"`. There the struct doc is
   a heading and a field doc is its row; no `# Arguments` or field-doc override.
7. **A bare name in `commands!` is a handle.** A local `Command` needs parentheses:
   `commands![(c)]`. External subcommands are allowlisted by absolute path, never `$PATH`.

## 12. clap to hicli

| clap | hicli |
|---|---|
| `#[derive(Parser)]`, `enum Commands`, `match` | `app! { globals: .., commands: [..] }` |
| `#[derive(Subcommand)]` variant and its arm | `#[command] fn`; its doc comment is the help |
| nested subcommand enum | `#[group] mod`, inline or `pub mod x;` in a file |
| `about` / `long_about` / `after_help` | doc comment: first paragraph / the rest / `# Examples` |
| `#[arg(long)] name: String` | `#[arg(long, metavar = "NAME")] struct Name(String)` |
| positional; `bool` flag | `#[arg(positional = "NAME")]`; `struct Flag(bool)` with `#[arg(long)]` |
| `Option<T>` / `Vec<T>` | `Option<T>` parameter, or `T(Option<V>)` / `T(Vec<V>)` |
| `ValueEnum`; `value_parser` | `#[derive(Choice, Arg)]`; `FromStr` on the type |
| `env`; `alias`/`hide` | `env = ".."`; `#[command(alias = "..")]` / `hide` |
| `#[command(flatten)]`; `global = true` | composed `#[derive(Arg)]` struct; `globals:` |
| per-arg help | the type's doc comment; `# Arguments` per command |
| `default_value` | no key: an `Option` defaulted in the body or `mk_ctx` |
| `version` / `about`; default subcommand | `.version(..)` / `.about(..)` on `App`; `#[command(default)]` |
| `clap_complete`; `get_matches_from` | `completion_script(shell, "bin")`; `App::parse(cli! { .. })` |
| `conflicts_with`, `requires`, `num_args` | not in hicli: `#[derive(Bpaf, Arg)]` |
