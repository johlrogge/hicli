# hicli

**Write a subcommand as a documented function. hicli adds the command line.**

hicli is a subcommand framework for Rust, built on [`bpaf`](https://docs.rs/bpaf).
A command is a function with a doc comment, and a group of commands is a module.
You don't write a central `enum Commands` or a `match` that dispatches it, and you
never write a command's name or help text twice. The functions stay plain Rust, so
you can call them and test them directly. The whole CLI can also be tested
in-process, without spawning a binary.

[![crates.io](https://img.shields.io/crates/v/hicli.svg)](https://crates.io/crates/hicli)
[![docs.rs](https://docs.rs/hicli/badge.svg)](https://docs.rs/hicli)

> 0.6.0, pre-1.0: a minor version may break things. Every break is listed in
> [`CHANGELOG.md`](CHANGELOG.md), and [Upgrading from 0.5](#upgrading-from-05)
> covers this release's. MSRV: Rust 1.99.

## Quick start

Most examples below build one CLI step by step. `playr` controls a networked music
player: it starts and stops playback, searches a library and manages music sources.
First, three commands and a global option:

```toml
[dependencies]
hicli = "0.6"
bpaf = "0.9"   # needed for #[derive(Bpaf)], used here on Globals
```

```rust
use std::fmt;
use std::process::ExitCode;
use std::str::FromStr;

use bpaf::Bpaf;
use hicli::{app, command, group, App, Arg};

/// Global options
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// The node to talk to
    #[bpaf(long, env("PLAYR_NODE"), argument("HOST"), fallback(Node::localhost()))]
    pub node: Node,
}

/// Built once, after parsing, and handed to every command.
pub struct Ctx {
    pub node: Node,
}

/// An audio output device
#[derive(Debug, Clone, Arg)]
#[arg(positional = "NAME")]
pub struct Output(pub Name);

/// Check that the node answers
#[command]
fn ping(ctx: &Ctx) {
    println!("pong from {}", ctx.node);
}

/// Playback control
#[group]
mod playback {
    use super::*;

    /// Start playback from the queue
    #[command]
    pub fn start(ctx: &Ctx) {
        println!("{}: playing", ctx.node);
    }

    /// Stop playback
    #[command]
    pub fn stop(ctx: &Ctx) {
        println!("{}: stopped", ctx.node);
    }

    /// Select an audio output device
    #[command]
    pub fn set_output(output: Output, ctx: &Ctx) {
        println!("{}: output is now {}", ctx.node, output.0);
    }
}

fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [ping, playback],
    }
}

fn main() -> ExitCode {
    app().run(|g| Ok(Ctx { node: g.node }))
}
```

`Name` and `Node` are the program's own types, shown in the next section.

```console
$ playr --help
Usage: playr [--node=HOST] COMMAND ...

Global options
        --node=HOST  The node to talk to
                     [env:PLAYR_NODE: N/A]

Available options:
    -h, --help       Prints help information

Available commands:
    ping             Check that the node answers
    playback         Playback control

$ playr playback --help
Playback control

Usage: playr playback COMMAND ...

Available options:
    -h, --help  Prints help information

Available commands:
    start       Start playback from the queue
    stop        Stop playback
    set-output  Select an audio output device

$ playr --node pi.local playback set-output hdmi
pi.local: output is now hdmi
$ PLAYR_NODE=pi.local playr playback stop
pi.local: stopped
$ playr playbak stop
Error: no such command or positional: `playbak`, did you mean `playback`?
$ playr playback set-output
Error: expected `NAME`, pass `--help` for usage information
$ echo $?
2
```

What each piece does:

- **`#[command]` on a function makes it a subcommand.** The name comes from the
  function name, with `_` turned into `-` (`set_output` becomes `set-output`), unless
  you write `#[command(name = "…")]`. The first paragraph of the doc comment is its
  help (see [Help text](#help-text)). The function itself is unchanged:
  `playback::stop(&ctx)` still works.
- **`#[group]` on a module makes it a group.** Its commands are the `#[command]`
  functions and `#[group]` modules inside it, in source order. There is no list to
  keep in sync. A command or group without a doc comment is a compile error.
- **An argument is a type.** `set_output` takes an `Output`, a newtype that derives
  `Arg`. `#[arg(positional = "NAME")]` says where it goes on the command line, and
  its doc comment is its help. Define it once and use it in every command that
  needs it. hicli has no `Arg` impls for `String`, `u64` and so on, so you always
  use a newtype ([ADR-006](docs/adr/006-an-argument-is-a-type.md)). Inside it is
  the value's own type, here a `Name`.
- **Global options are a `Bpaf` struct.** They are accepted before or after the
  subcommand (`playr playback stop --node pi.local`), except an external subcommand,
  which needs them before its name (see the rules below). `env(..)` makes an
  environment variable the fallback, and the doc comment on the struct becomes the
  `Global options` heading.
- **Never put a secret in a `Globals` field, not even with `env(..)`.** When the
  variable is set, `--help` prints its value: `PLAYR_NODE=x playr --help` shows
  `[env:PLAYR_NODE = "x"]`.
- **The context `&Ctx`** is built once from the globals, by the closure you pass to
  `run`, and every command that takes `&Ctx` receives it. Put connections and
  configuration in it. Read secrets from the environment in that closure. A
  connection that is slow or can fail is opened by the first command that asks
  for it; see [A context that connects only when asked](#a-context-that-connects-only-when-asked).
- **Exit codes:** `0` on success, `1` for a runtime failure, `2` for a usage error.
  `run` returns the `ExitCode`. It never calls `process::exit`.

### Bad values never reach a command

The newtype says how an argument is spelled, and a `FromStr` says what a valid value
is. The newtype's own `FromStr` is used if it has one; otherwise the `FromStr` of the
type inside it. Either way the rules live in one place, and every command receives a
value that is already valid. `Output` has no `FromStr` of its own, so its word is
parsed by `Name`'s:

```rust
/// Lowercase letters, digits and `-`, such as `hdmi` or `bandcamp`.
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);

impl FromStr for Name {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if s.is_empty() || !s.chars().all(allowed) {
            return Err("a name uses only a-z, 0-9 and -".to_string());
        }
        Ok(Name(s.to_string()))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}
```

`Node`, the `--node` value, is written the same way: a host name with an optional
`:port`. A value that doesn't parse is a usage error, and no command runs. The
message names the value, the argument it was given to, and what is wrong:

```console
$ playr playback set-output HDMI
Error: couldn't parse `HDMI`: `NAME`: a name uses only a-z, 0-9 and -
$ echo $?
2
$ playr --node pi.local:http ping
Error: couldn't parse `pi.local:http`: `http` is not a port number
```

Known gap: an argument spelled by bpaf, such as a `Globals` field like `--node`
above or a field of a `#[derive(Bpaf)]` struct, is not named, only its value. With
several such values of the same type, the user has to work out which one failed
([#26](https://github.com/johlrogge/hicli/issues/26)). Types spelled with `#[arg(…)]`
are always named.

A newtype over a type that isn't yours, such as `NonZeroUsize` or `PathBuf`, can
carry the rule itself, because its own `FromStr` comes first:

```rust
/// How many to take
#[derive(Debug, Clone, Arg)]
#[arg(long = "limit", short = 'l', metavar = "N")]
pub struct Limit(pub NonZeroUsize);

impl FromStr for Limit {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Limit).map_err(|_| "must be at least 1")
    }
}
```

`--limit 0` and `-l 0` both fail with ``couldn't parse `0`: `--limit`: must be at
least 1``: the name is the argument's spelling, not what was typed.

A value with no rules, like the free-text search query below, can stay a `String`
inside its newtype. Paths are `PathBuf`, and choices are enums (see
[Choice values](#choice-values)).

### What a `FromStr` error should say

Say what is wrong with the word, as a phrase that reads after the argument's name:
`must be at least 1`, not `'0' must be a whole number of at least 1`. Leave out the
word, which bpaf prints, and the argument, which hicli prints. The error type can be
anything that implements `Display`: a `String`, a `&str`, any `std::error::Error`.

## Growing the example

The following sections add to `playr`. The complete program, with tests, is
[`examples/playr.rs`](examples/playr.rs).

### Options, switches and positionals

An argument type that holds one value is spelled on the type, with `#[arg(…)]`.
`playr search` takes an optional free-text query, and `source … check-updates`
takes a switch:

```rust
/// Free-text query over title, artist and album
#[derive(Debug, Clone, Arg)]
#[arg(positional = "QUERY")]
pub struct Query(pub Option<String>);

/// Re-download anything that is out of date
#[derive(Debug, Clone, Arg)]
#[arg(long = "apply")]
pub struct Apply(pub bool);
```

The keys are:

| key | means |
|---|---|
| `positional = "QUERY"` | a positional, shown as `QUERY` |
| `long = "apply"`, `short = 'y'` | an option, `--apply` or `-y`, alone or together |
| `metavar = "DAYS"` | an option's value, shown as `--added=DAYS`; required unless the value is a `bool` |
| `complete = f`, `files`, `dirs` | completion (see [Shell completion](#shell-completion)) |

A bare `positional` or `long` is named after the type: `SortBy` becomes `SORT-BY`
or `--sort-by`. What is inside decides the rest: `Option<T>` is optional, `Vec<T>`
repeats, a `bool` with a name is a switch, and anything else is one word. The
first paragraph of the type's doc comment is its help row, so the doc comment is
required. These five keys are all there is, on purpose. A type that needs more,
such as a fallback, is spelled with bpaf.

Which `FromStr` parses the word:

| the type | parsed by |
|---|---|
| `Output(Name)` | `Output`'s own `FromStr` if it has one, else `Name`'s |
| `Added(Option<DaysBack>)`, `T(Vec<I>)` | the inner type's, once per word |
| `AudioFile(PathBuf)` | `AudioFile`'s own if it has one, else the path as given |
| a struct with several fields | its own, which it must have |
| a `Choice` enum | the one `#[derive(Choice)]` writes |

A struct with several fields can be spelled too, when it is one word that its own
`FromStr` takes apart. An upload that is either an audio file or a ZIP archive:

```rust
/// An audio file or a ZIP archive
#[derive(Debug, Clone, PartialEq, Eq, Arg)]
#[arg(positional = "FILE", complete = files)]
pub struct Upload {
    /// Where the file is.
    pub path: PathBuf,
    /// Whether it is a ZIP archive.
    pub zip: bool,
}

impl FromStr for Upload {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let zip = if s.ends_with(".zip") {
            true
        } else if s.ends_with(".flac") {
            false
        } else {
            return Err("not audio or a ZIP archive");
        };
        Ok(Upload {
            path: PathBuf::from(s),
            zip,
        })
    }
}
```

`a.txt` fails with ``couldn't parse `a.txt`: `FILE`: not audio or a ZIP archive``.
Without the `FromStr`, it is a compile error. (Don't name a command after the type
in snake_case, here `upload`: the derive writes a function `upload()` beside the
type, and the two clash.)

Several options that belong together can share one struct, spelled with
`#[derive(Bpaf)]` as in bpaf. This is also how `Globals` is written. Each field's
doc comment is its help row. `Bpm` accepts 20 to 300, written like `Name`:

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Filters {
    /// Only tracks by this artist
    #[bpaf(long, argument("NAME"))]
    pub artist: Option<String>,
    /// Only tracks at this tempo
    #[bpaf(long, argument("BPM"))]
    pub bpm: Option<Bpm>,
    /// Only tracks that were never played
    #[bpaf(long)]
    pub unplayed: bool,
}
```

Document the fields of a `#[derive(Bpaf)]` struct, not the struct. bpaf turns a
doc comment on the struct into a section heading. That is how `Global options`
appears above.

A command's parameters are parsed in the order they are declared. **Declare the
types that contain positionals last.** bpaf requires this, and a debug build checks it
the first time it runs. A release build doesn't check it up front: the same mistake
panics when `--help` is rendered. The rule is only about parameter order: on the
command line, flags can go anywhere.

### Values that start with `-`

`playr search --added=-7` means "added in the last 7 days". Write such a value glued
to its option with `=`. A plain `argument` reads it, and the type inside decides what
is valid: `DaysBack` accepts `-7` and rejects `7`, `-0` and `-x`.

```rust
/// Only tracks added in the last DAYS days, e.g. -7
#[derive(Debug, Clone, Arg)]
#[arg(long = "added", metavar = "DAYS")]
pub struct Added(pub Option<DaysBack>);
```

As two words, `-7` reads as a flag, and bpaf says how to write it instead:

```console
$ playr search --added=-7
localhost: search None artist=None bpm=None unplayed=false added=Some(DaysBack(7))
$ playr search --added -7
Error: `--added` requires an argument `DAYS`, got a flag `-7`, try `--added=-7` to use it as an argument
$ playr search --added=-x
Error: couldn't parse `-x`: `--added`: not a number of days back, such as -7
```

hicli keeps to the standard rule rather than special-casing hyphen values: a parser
that sometimes takes `-7` as a value and sometimes as a flag surprises users.

### A group's default command

`playr search jazz` searches, and `playr search fact-values-for genre` lists the
genres. When a group has a `#[command(default)]`, that command runs if no other
command in the group is named:

```rust
/// Search the library
#[group]
mod search {
    use super::*;

    /// List every value stored for a fact type
    #[command]
    pub fn fact_values_for(t: FactType, ctx: &Ctx) {
        println!("{}: values for {}", ctx.node, t.0);
    }

    /// Search by text and filters
    #[command(default)]
    pub fn tracks(added: Added, f: Filters, q: Query, ctx: &Ctx) {
        println!(
            "{}: search {:?} artist={:?} bpm={:?} unplayed={} added={:?}",
            ctx.node, q.0, f.artist, f.bpm, f.unplayed, added.0
        );
    }
}
```

```console
$ playr search --added=-7 --artist CBL techno
localhost: search Some("techno") artist=Some("CBL") bpm=None unplayed=false added=Some(DaysBack(7))
$ playr search fact-values-for genre
localhost: values for genre
$ playr search -- fact-values-for
localhost: search Some("fact-values-for") artist=None bpm=None unplayed=false added=None
$ playr search --help
Search the library

Usage: playr search (COMMAND ... | [--added=DAYS] [--artist=NAME] [--bpm=BPM] [--unplayed] [QUERY])

Search by text and filters
        --added=DAYS   Only tracks added in the last DAYS days, e.g. -7
        --artist=NAME  Only tracks by this artist
        --bpm=BPM      Only tracks at this tempo
        --unplayed     Only tracks that were never played
    QUERY              Free-text query over title, artist and album

Available options:
    -h, --help         Prints help information

Available commands:
    fact-values-for    List every value stored for a fact type
```

The default command has no name of its own. To search for a word that is also a
command name, put `--` before it. Groups can nest, so one module can hold another.
Here `library archive` lists the archive and `library archive empty` empties it:

```rust
/// Library maintenance
#[group]
mod library {
    use super::*;

    /// Files that duplicate merges moved aside
    #[group]
    pub mod archive {
        use super::*;

        /// Permanently delete everything in the archive
        ///
        /// # Arguments
        ///
        /// * `yes` - Confirm: the files cannot be recovered
        #[command]
        pub fn empty(yes: Yes, ctx: &Ctx) -> Outcome {
            if !yes.0 {
                return Err(Failure::usage(
                    "this cannot be undone: pass --yes to confirm",
                ));
            }
            println!("{}: archive emptied", ctx.node);
            Ok(())
        }

        /// List the archive
        #[command(default)]
        pub fn list(ctx: &Ctx) {
            println!("{}: 3 files in the archive", ctx.node);
        }
    }
}
```

```console
$ playr library archive
localhost: 3 files in the archive
$ playr library archive emtpy
Error: no such command or positional: `emtpy`, did you mean `empty`?
$ playr library archive empty
Error: this cannot be undone: pass --yes to confirm
$ echo $?
2
$ playr library archive empty --yes
localhost: archive emptied
$ playr library archive --help
Files that duplicate merges moved aside

Usage: playr library archive [COMMAND ...]

List the archive

Available options:
    -h, --help  Prints help information

Available commands:
    empty       Permanently delete everything in the archive
```

The `# Arguments` section in `empty`'s doc comment is explained in
[Overriding an argument's help](#overriding-an-arguments-help).

Limits: one default per group; none at the top level or in an `args`/`curried`
group; a default takes no `name`, `alias` or `hide`, and its doc comment is one
paragraph (it has no help page of its own). That paragraph titles the default's
options in the group's `--help`, as with `search` above; a default without options,
like `list` here, has it shown under `Usage:`, and the usage reads
`[COMMAND ...]` because naming a command is optional. See
[ADR-011](docs/adr/011-curried-commands-and-default-commands.md).

### Object, then verb: `source <name> check-updates`

`playr source list` lists the sources, and `playr source bandcamp check-updates`
acts on one of them. Here a value (the source name) comes between the group and
the command. Use `#[group(args = T)]` for the group and `#[command(curried)]` for
each command that receives the value, which must be its first parameter:

```rust
/// A source on the node: bandcamp, beatport or upload
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SOURCE", complete = complete_sources)]
pub struct SourceName(pub Name);

/// Music sources
#[group(args = SourceName)]
mod source {
    use super::*;

    /// List the sources on the node
    #[command]
    pub fn list(ctx: &Ctx) {
        println!("{}: bandcamp, beatport, upload", ctx.node);
    }

    /// Check the source's collection for stale items
    #[command(curried)]
    pub fn check_updates(s: SourceName, apply: Apply, ctx: &Ctx) {
        println!("{}: checking {} (apply: {})", ctx.node, s.0, apply.0);
    }
}
```

```console
$ playr source bandcamp check-updates --apply
localhost: checking bandcamp (apply: true)
$ playr source list
localhost: bandcamp, beatport, upload
$ playr source --help
Music sources

Usage: playr source (COMMAND ... | SOURCE COMMAND ...)

Commands for SOURCE:
    check-updates  Check the source's collection for stale items

Available positional items:
    SOURCE         A source on the node: bandcamp, beatport or upload

Available options:
    -h, --help     Prints help information

Available commands:
    list           List the sources on the node
```

A source with no command says what is missing, and lists the commands that can
follow it; a misspelled one gets a "did you mean":

```console
$ playr source bandcamp
Error: expected a command after `bandcamp`: `check-updates`
```

Commands without the `curried` mark, like `list`, are matched first, so a source
can't be named `list`. A curried command whose first parameter has a different type
is a compile error. A nested group that also receives the value is
`#[group(curried = SourceName)]`. One known limit is surprising: `source <TAB>` with
nothing typed offers `list`, the sources and also `check-updates`, which is not valid
until a source is typed. For the others, such as `source NAME --help` showing the group's
page, see [ADR-011](docs/adr/011-curried-commands-and-default-commands.md).

### Errors and `?`

A command can return whatever it already returns: `()`, `Outcome`, `io::Result<()>`,
`anyhow::Result<()>`, `color_eyre::Result<()>`, `Result<(), Box<dyn Error>>`.
`Box<dyn Error + Send + Sync>` is not accepted (the standard library can't convert
it); return `Box<dyn Error>`, or `anyhow`/`eyre`. Any error becomes a runtime failure
(exit 1). The message is the whole error chain, outermost first:

```rust
use color_eyre::eyre::WrapErr;

/// Upload an audio file to the library
#[command]
fn upload(file: AudioFile, ctx: &Ctx) -> color_eyre::Result<()> {
    let bytes = std::fs::read(&file.0)
        .wrap_err("reading the file")
        .wrap_err_with(|| format!("uploading {}", file.0.display()))?;
    println!("{}: uploaded {} bytes", ctx.node, bytes.len());
    Ok(())
}
```

```console
$ playr upload missing.flac
Error: uploading missing.flac: reading the file: No such file or directory (os error 2)
$ echo $?
1
```

hicli treats an error as a usage error (exit 2) only when it says so. bpaf reports
arguments that fail to parse as usage errors, and a command can return
`Failure::usage(..)` from an `Outcome`, like `archive empty` above. hicli never
inspects an error to choose an exit code. For any other code, return
`Ok(Success::code(n))`. See [ADR-012](docs/adr/012-command-errors-convert-through-into-failure.md).

### A context that connects only when asked

`run` builds the context before the command runs, and it doesn't tell the closure
which command that is. Keep the closure cheap: store what was parsed. Anything
slow or fallible, such as a connection, goes in a `OnceLock` and is opened by the
first command that asks for it:

```rust
/// Built once, after parsing, and handed to every command.
pub struct Ctx {
    pub node: Node,
    session: OnceLock<Session>,
}

impl Ctx {
    /// The session with the node, opened the first time a command asks.
    pub fn session(&self) -> Result<&Session, Failure> {
        // `OnceLock::get_or_try_init` is unstable.
        if let Some(session) = self.session.get() {
            return Ok(session);
        }
        let opened = Session::open(&self.node)?;
        Ok(self.session.get_or_init(|| opened))
    }
}

fn mk_ctx(g: Globals) -> Result<Ctx, Failure> {
    Ok(Ctx {
        node: g.node,
        session: OnceLock::new(),
    })
}
```

A command that talks to the node says so, with `?`:

```rust
/// Check that the node answers
#[command]
fn ping(ctx: &Ctx) -> Outcome {
    let session = ctx.session()?;
    println!("pong from {}", session.node);
    Ok(())
}
```

`sort` and `generate-completions` never call `session()`, so they work when no node
answers. If opening fails, the error comes from the command that asked: exit 1, or
2 if it is a `Failure::usage`. The list of commands that need a connection is not
written anywhere; each one says it where it calls `session()`. A test can check
that a command never connected:

```rust
#[test]
fn sort_never_opens_a_session() {
    let (globals, invocation) = app().parse(cli! { sort bpm }).expect("parses");
    let ctx = mk_ctx(globals).expect("building the context is cheap");
    invocation.run(&ctx).expect("sorts");
    assert!(ctx.session.get().is_none(), "sort opened a session");
}
```

hicli has no API for this, and doesn't need one. See
[ADR-014](docs/adr/014-a-lazy-context-needs-no-api.md).

### Choice values

`playr sort bpm` takes one of a fixed set of words. Write an enum whose variants
have no fields and derive `Choice`. Add `Arg` with `#[arg(positional = "KEY")]`,
and the enum itself is the argument:

```rust
/// What to sort by
//
// A choice is the argument: its names parse, complete and appear in errors.
#[derive(Debug, Clone, Copy, PartialEq, Choice, Arg)]
#[arg(positional = "KEY")]
pub enum SortKey {
    /// Beats per minute
    Bpm,
    /// Track title
    Title,
    /// Artist name
    Artist,
}
```

`#[derive(Choice)]` writes `FromStr` and `Display`. Each variant is spelled as its
name in kebab-case (`PlayCount` becomes `play-count`), or as you name it with
`#[choice(name = "…")]`. Matching is exact. A word that is none of them is a usage
error that lists the names, and suggests one if it is close:

```console
$ playr sort key
Error: couldn't parse `key`: `KEY`: expected `bpm`, `title` or `artist`
$ playr sort bmp
Error: couldn't parse `bmp`: `KEY`: expected `bpm`, `title` or `artist`; did you mean `bpm`?
```

The names complete on their own, with no completer to write. `sort <TAB>` offers
`bpm`, `title` and `artist` (and the `--descending` flag), and `sort t<TAB>` offers
`title`. A shell that shows
descriptions shows each variant's doc comment beside it. The help row ends with
the names, in bold on a terminal: `KEY  What to sort by [bpm, title, artist]` (see
`sort --help` [below](#help-text)). So don't list them in the doc comment too; they
would appear twice. A choice inside a newtype completes and lists its names the
same way: `struct SortOpt(pub Option<SortKey>)` with
`#[arg(long = "sort", metavar = "KEY")]` is an optional `--sort=KEY`.

### Help text

All help comes from doc comments, and one rule reads them all: **the first
paragraph is the help.** That is every line up to the first blank line, joined by
spaces, so a long help can wrap onto a second `///` line. What comes after the
first blank line depends on what the doc comment is on:

- On a **command or group**, it is the details, printed under the usage line of
  its `--help`. An `# Examples` section is printed at the end, and an
  `# Arguments` section (see [below](#overriding-an-arguments-help)) is read, not
  printed.
- On an **argument type spelled with `#[arg(…)]`**, it is rustdoc only and never
  printed. Write notes for developers there.
- On a **`Choice` variant**, the first paragraph is its completion description,
  and the rest is rustdoc only.

`sort` takes the `SortKey` above and a switch:

```rust
/// Sort descending (Z→A, high→low)
#[derive(Debug, Clone, Arg)]
#[arg(short = 'd', long = "descending")]
pub struct Descending(pub bool);
```

Its doc comment has a help, details and examples. Put a blank `///` line between
the help and the details:

```rust
/// Sort track ids read from stdin
///
/// Reads one track id per line and prints them sorted.
///
/// # Examples
///
/// ```sh
/// playr search --artist CBL | playr sort bpm --descending
/// ```
#[command]
fn sort(desc: Descending, by: SortKey) {
    println!("sorting by {by}, descending: {}", desc.0);
}
```

```console
$ playr sort --help
Sort track ids read from stdin

Usage: playr sort [-d] KEY

Reads one track id per line and prints them sorted.

Available positional items:
    KEY               What to sort by [bpm, title, artist]

Available options:
    -d, --descending  Sort descending (Z→A, high→low)
    -h, --help        Prints help information

Examples:
  playr search --artist CBL | playr sort bpm --descending
```

`sort` takes no `&Ctx`, so it is generic over the context and works in any app. Use
`#[command(alias = "ls")]` to add another name and `#[command(hide)]` to keep a
command out of help and completion; `#[group(alias = "…")]` and `#[group(hide)]` do
the same for a group. The text above the top-level usage line and `--version` are set on the
app:

```rust
fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [ping, playback, search, library, source, upload, sort, generate_completions],
    }
    .about("playr: control the music player")
    .version(env!("CARGO_PKG_VERSION"))
}
```

### Overriding an argument's help

An argument type's help is written once, on the type, and every command that takes
it shows it. A command that needs to say something more specific lists the
parameter in an `# Arguments` section of its doc comment. `Yes` says "Do not ask
for confirmation", and `library archive empty` (above) says instead:

```rust
/// Permanently delete everything in the archive
///
/// # Arguments
///
/// * `yes` - Confirm: the files cannot be recovered
#[command]
pub fn empty(yes: Yes, ctx: &Ctx) -> Outcome {
```

```console
$ playr library archive empty --help
Permanently delete everything in the archive

Usage: playr library archive empty [-y]

Available options:
    -y, --yes   Confirm: the files cannot be recovered
    -h, --help  Prints help information
```

- Each entry is `` * `name` - help `` (a `-` bullet works too), named after the
  parameter. An entry can continue on indented lines, which are joined with spaces.
- Only the help row changes. Usage, completion and errors stay the type's, and the
  section is not printed as part of the details.
- It works for argument types spelled with `#[arg(…)]`, and for hand-written ones
  that implement `ArgHelp`. An entry for a `#[derive(Bpaf)]` type is a compile
  error, because bpaf builds that help.
- It is a compile error to name something that is not a parsed parameter of the
  command (the `&Ctx`, a curried value, a misspelling), to name one twice, or to
  write the section on a group or on a `#[command(default)]`.

This is the only way a command says anything about its arguments. There are no
parameter attributes.

### Colour

On a terminal, `run` prints help and errors in colour: headings and option names in
bold, metavariables underlined, and `Error: ` in bold red. It needs both stdout and
stderr to be terminals, so `playr --help | less` and `playr --help 2>/dev/null` print
plain text. `NO_COLOR=1` turns colour off and `FORCE_COLOR=1` forces it on; there is
no `--color` flag. To never print colour, depend on
`hicli = { version = "0.6", default-features = false }`. For bright colours, add
`bpaf = { version = "0.9", features = ["bright-color"] }`. `App::parse` and the
snapshot tests built on it always get plain text without the `Error: ` prefix. See
[ADR-015](docs/adr/015-coloured-output-through-print-message.md).

### Shell completion

Completion is attached to the argument type, so every command that takes a
`SourceName` completes source names:

```rust
/// Runs on every TAB, with the word typed so far: never fails, and filters.
fn complete_sources(typed: &str) -> Vec<Candidate> {
    ["bandcamp", "beatport", "upload"]
        .into_iter()
        .filter(|s| s.starts_with(typed))
        .map(|s| (s.to_string(), None))
        .collect()
}
```

`#[arg(complete = complete_sources)]` on the type (shown above) connects the two.
The completer is handed the word as typed, before it is parsed, so it runs from the
first keystroke, even for a value such as a number that an empty or half-typed word
is not. A `Choice` completes with no completer at all. For paths, use
`#[arg(complete = files)]` or `#[arg(complete = dirs)]`, which let the shell complete
them. **Your completer must filter by the typed prefix itself.** bpaf passes its
results to the shell unchanged.

Command names complete automatically. To install completion, render the script from a
hidden command. `hicli::Shell` is a `Choice`, so `generate-completions <TAB>` offers
`bash`, `zsh` and `fish`, and its help row lists them
(`SHELL  A shell to print completions for [bash, zsh, fish]`):

```rust
/// A shell to print completions for
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SHELL")]
pub struct ShellArg(pub hicli::Shell);

/// Print the shell completion script
#[command(hide)]
fn generate_completions(shell: ShellArg) {
    print!("{}", hicli::completion_script(shell.0, "playr"));
}
```

```console
$ playr generate-completions zsh > ~/.zfunc/_playr
```

### Testing

The parser has no side effects, so the whole CLI can be tested in-process. You don't
need a subprocess or golden files:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hicli::{cli, cli_test, exit_code_for};

    cli_test! {
        app: app();

        stop_is_under_playback: { playback stop } => |r| {
            let (_globals, invocation) = r.expect("parses");
            assert_eq!(invocation.path(), ["playback", "stop"]);
        },
        node_after_the_command: { playback stop --node "pi.local" } => |r| {
            let (globals, _) = r.expect("parses");
            assert_eq!(globals.node.host, "pi.local");
        },
        set_output_needs_a_name: { playback set-output } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("NAME"));
        },
        output_names_are_checked: { playback set-output HDMI } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert_eq!(
                e.unwrap_stderr(),
                "couldn't parse `HDMI`: `NAME`: a name uses only a-z, 0-9 and -"
            );
        },
    }

    #[test]
    fn emptying_the_archive_needs_yes() {
        assert_eq!(
            app().run_with_args(cli! { library archive empty }, mk_ctx),
            2
        );
        assert_eq!(
            app().run_with_args(cli! { library archive empty --yes }, mk_ctx),
            0
        );
    }

    #[test]
    fn source_names_complete() {
        assert_eq!(app().complete(cli! { source b }), ["bandcamp", "beatport"]);
    }

    #[test]
    fn the_command_is_still_a_function() {
        let ctx = Ctx {
            node: Node::localhost(),
            session: OnceLock::new(),
        };
        assert!(upload(AudioFile("/no/such/file.flac".into()), &ctx).is_err());
    }
}
```

- `cli! { … }` turns the words you would type into an argv, and `cli_test!` writes
  one `#[test]` per line. Words are separated by whitespace, as in a shell:
  `set-output`, `--dry-run` and `--since=-7` are one word each, and `"My Event"` is
  one word with a space in it.
- `App::parse` returns the parsed globals and the selected command, in that order, or
  the error that would be printed. `App::run_with_args` runs everything and returns the exit code
  that `main` would. `App::complete` returns what the shell would offer.
- Commands are ordinary functions, so you can call them directly.
- `App::run` starts every usage line with the binary's name (`Usage: playr playback
  stop`); the test seams don't know it, so `.bin_name("playr")` makes snapshot tests
  show what `run` prints.

## Rules and papercuts

1. **MSRV is Rust 1.99.** `#[group] pub mod x;` needs attribute macros on file-module
   declarations, stable since 1.99.
2. **If you use `#[derive(Bpaf)]`, add `bpaf = "0.9"` to your own dependencies.**
   `#[derive(Bpaf)]` generates `::bpaf::` paths. Cargo still builds a single copy of
   bpaf. Types spelled with `#[arg(…)]` don't need it; a `Globals` struct written
   with `#[derive(Bpaf)]` does.
3. **Declare positional parameters last** (see above).
4. **There are no `Arg` impls for primitives.** Give each value a newtype, with the
   value's own type inside.
5. **Completers filter their own candidates.** bpaf passes them to the shell
   unchanged. A completer on a type spelled with `#[arg(…)]` is handed the raw word,
   so it runs on every keystroke. On a `#[derive(Bpaf, Arg)]` type with
   `#[arg(complete = …)]`, it is handed the value that already parsed instead, so a
   value checked by `FromStr`, such as a `Name` or a number, doesn't complete until
   the partial word parses, and never completes from an empty word. Spell the type
   with `#[arg(…)]` to fix that.
6. **`#[arg(complete = …)]` on a `#[derive(Bpaf, Arg)]` type goes only on a
   single-field one.** On a multi-field bpaf struct it is a compile error. A struct
   with several fields spelled with `#[arg(…)]` is one word, so it completes like
   any other.
7. **`Globals` doesn't derive `Arg`.** It is parsed once at the top level.
8. **A `#[group] pub mod x;` that lives in its own file must be declared at the top
   level of a file**, not inside an inline module.
9. **The attribute keys:**
   - `#[command(name = "…", alias = "…", hide)]`, plus `default` and `curried` (above).
   - `#[group(alias = "…", hide)]`, plus `args` and `curried` (above). A group has no
     `name`: it is the module's.
   - `#[group(ctx = T)]` names a group's context. Without it, the group takes the
     first `&T` among its commands, and a command with no `&T` is generic over `Ctx`.
   - `#[arg(positional = "…", long = "…", short = '…', metavar = "…", complete = …)]`
     on an argument type, and `#[choice(name = "…")]` on a `Choice` variant.
   - A doc comment's `# Examples` and `# Arguments` sections.
10. **A bare name in `commands!` names a handle.** A local `Command` value must be
    parenthesised, `commands![(c)]`. A macro of your own that forwards a list to
    `app!` or `commands!` should match it as `$($c:tt)*`, not `$c:expr`.
11. **External subcommands** (`hicli::external(ExternalCommand::new(…))`):
    - Inside a group, write the group's handle by hand,
      `#[doc(hidden)] pub fn __hicli_<name>() -> Command<Ctx>`, returning
      `hicli::group(…)`. Discovery finds items, not values, so an external can't be a
      discovered child.
    - Globals go before the external's name. Everything after the name is forwarded
      to the binary verbatim.
    - A `--` right after the external's name is eaten by bpaf; the binary only sees
      the words after it.
    - See `ExternalCommand` on [docs.rs](https://docs.rs/hicli) for the rest.

## Upgrading from 0.5

Most code builds unchanged, but help and error text changes. Check these:

- **Re-bless your help and error snapshots.** An error from a type spelled with
  `#[arg(…)]` now names the argument after the value:
  ``couldn't parse `bmp`: `KEY`: expected `bpm`, `title` or `artist`; did you mean
  `bpm`?``. The help row of a `Choice`, and of a type that holds one, ends with its
  names: `KEY  What to sort by [bpm, title, artist]`. Types spelled with
  `#[derive(Bpaf)]`, and hand-written `Arg` impls, are unchanged.
- **Remove choice names you listed by hand.** A doc comment such as
  `/// What to sort by: bpm, title or artist` now shows the names twice. Write
  `/// What to sort by`.
- **A type's own `FromStr` now runs.** In 0.5, `#[arg(…)]` on `Limit(NonZeroUsize)`
  parsed with `NonZeroUsize`'s `FromStr` and ignored `Limit`'s, so the type's rule
  never ran. Now `Limit`'s runs, and it may reject words that 0.5 accepted. A
  `FromStr` error that repeats the word or names the argument now says them twice:
  shorten it to what is wrong (see
  [What a `FromStr` error should say](#what-a-fromstr-error-should-say)).
- **A newtype where neither the type nor its field implements `FromStr`** is now
  E0599, ``the method `parse_word` exists for reference
  `&hicli::__private::FromStrOf<Wrap, Opaque>`, but its trait bounds were not
  satisfied``, with notes naming both bounds (`Wrap: FromStr`, `Opaque: FromStr`).
  It was E0277. Implement `FromStr` for either one.
- **`#[arg(positional|long|short …)]` on a struct with several fields** no longer
  says to use `#[derive(Bpaf)]`. It is spelled as one word when the struct
  implements `FromStr`, and is E0277 (`T: FromStr`) when it doesn't.

The full list is in [`CHANGELOG.md`](CHANGELOG.md).

## Upgrading from 0.4

Most code builds unchanged. Check these:

- **The help is the first paragraph, not the first line.** Every line up to the first
  blank line is now joined into the help, for commands, groups, a
  `#[command(default)]` and `Choice` variants. A doc comment whose second line
  directly followed the first used to show that line as the start of the details;
  now it is part of the help. **Put a blank `///` line between the help and the
  details:**

  ```rust
  /// Sort track ids read from stdin
  ///
  /// Reads one track id per line and prints them sorted.
  ```

- **`# Arguments` (or `Arguments:`) in a command's doc comment is now read.** It
  starts the section described in
  [Overriding an argument's help](#overriding-an-arguments-help), so it is no longer
  printed as details, and its entries, `` * `name` - help ``, must name parameters of
  the command. rustdoc's habit of documenting parameters this way
  meets hicli here: an entry for a `#[derive(Bpaf)]` type is a compile error
  (E0277). Remove the entry, or spell the type with `#[arg(…)]`. An `# Arguments`
  heading after `# Examples` is still examples text.
- **`Shell`'s error type and text.** `"tcsh".parse::<Shell>()` fails with
  `UnknownChoice`, which displays ``expected `bash`, `zsh` or `fish` `` (and
  `"bsah"` adds ``; did you mean `bash`?``). It used to be ``unknown shell `tcsh`: expected one of `bash`, `zsh`,
  `fish` ``. `UnknownShell` is a deprecated alias of `UnknownChoice`. It was a tuple
  struct, so code that built or matched `UnknownShell(…)` no longer compiles.
- **`cli!` splits words only at whitespace**, as a shell does. In tests, a few
  spellings now read differently: `-- foo` is `["--", "foo"]` (was `["--foo"]`),
  `a - b` is three words and `- 5` two, `"a""b"` is `["ab"]`, and a number literal is
  kept as written (`0x10` stays `0x10`). A bracketed group (`[a]`) and a byte string
  (`b"a"`) are compile errors; quote them as `"[a]"`. See
  [ADR-017](docs/adr/017-cli-words-are-whitespace-separated-tokens.md).
- **`#[arg(…)]` on a `#[derive(Bpaf)]` type** rejects `metavar` (it needs
  `positional`, `long` or `short`, which switch to spelled mode), and a second
  `complete` gets its own message.

The full list is in [`CHANGELOG.md`](CHANGELOG.md).

### Moving an argument type to `#[arg(…)]`

This is optional, and each type can move on its own: the two forms work side by side.
A one-value `#[derive(Bpaf, Arg)]` newtype:

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_sources)]
pub struct SourceName(
    /// A source on the node: bandcamp, beatport or upload
    #[bpaf(positional("SOURCE"))]
    pub Name,
);

fn complete_sources(typed: &SourceName) -> Vec<Candidate> {
    // … filter by typed.0.to_string()
}
```

becomes:

```rust
/// A source on the node: bandcamp, beatport or upload
#[derive(Debug, Clone, Arg)]
#[arg(positional = "SOURCE", complete = complete_sources)]
pub struct SourceName(pub Name);

fn complete_sources(typed: &str) -> Vec<Candidate> {
    // … filter by typed
}
```

1. Move the field's doc comment onto the type. Its first paragraph is the help.
2. Replace `Bpaf` in the derive, and the `#[bpaf(…)]` attribute, with `#[arg(…)]`:
   - `positional("SOURCE")` becomes `positional = "SOURCE"`;
   - `long("added"), argument("DAYS")` becomes `long = "added", metavar = "DAYS"`;
   - `short('y'), long("yes"), switch` becomes `short = 'y', long = "yes"` on a
     `bool`;
   - drop `optional` and `many`: an `Option<T>` or `Vec<T>` inside says so.
3. A completer takes the word, `&str`, instead of the parsed value. It now also runs
   on an empty or half-typed word.
4. A hand-written choice enum with its own `FromStr`, wrapped in a newtype, can
   become `#[derive(Choice, Arg)]` with `#[arg(positional = "KEY")]`, as in
   [Choice values](#choice-values). Its error text becomes the one shown there.

Usage lines and parsing stay the same, and so does the help row of a one-paragraph
doc comment (only the first paragraph is printed now). The derive still writes the
snake_case constructor (`source_name()`), so a `#[bpaf(external(source_name))]`
that embeds the type elsewhere keeps working. Keep `#[derive(Bpaf)]` for a type
with several options or positionals, and for one that needs another bpaf attribute, such as
`fallback`, `env` or a guard. Your crate needs the direct `bpaf` dependency only
while something still uses `#[derive(Bpaf)]`.

## Upgrading from 0.1

Most code builds unchanged. Two things can break:

- **E0119, conflicting `From` impls:** delete your `impl From<MyError> for Failure`.
  hicli now converts every error type that implements `std::error::Error`, and keeps
  the whole chain.
- **E0282, type annotations needed:** a closure passed by hand to `hicli::command`
  or `hicli::curried_command` that returns `Ok(())` needs `-> Outcome` written out. `#[command]` functions are
  not affected.

The full list is in [`CHANGELOG.md`](CHANGELOG.md#breaking--upgrading-from-01).

## Going further

- **API reference:** [docs.rs/hicli](https://docs.rs/hicli).
- **Runnable examples:** [`examples/playr.rs`](examples/playr.rs) (this README's
  program; [`examples/playr_minimal.rs`](examples/playr_minimal.rs) is the Quick
  start), [`examples/demo.rs`](examples/demo.rs) (groups inline),
  [`examples/files/`](examples/files/) (one file per group), `just demo --help`.
- **External subcommands**, git style: mount a separate binary as a command by
  absolute path. It still appears in help and completion, and hicli never scans
  `$PATH`. See the rules above and `ExternalCommand` on docs.rs.
- **Writing a command by hand:** `#[command]` and `#[group]` expand to calls to
  public functions such as `hicli::command`, `hicli::group`,
  `hicli::curried_command`, `hicli::group_args` and `Node::by_default`. Anything the
  macros generate, you can also write yourself.
- **Coming from clap:** the help layout is bpaf's, not clap's. Globals are parsed once
  and can't be overridden per subcommand. A one-word argument is spelled with
  `#[arg(…)]` on its type, and anything bigger with bpaf's own attributes.
  `--help` exits 0 and usage errors exit 2.
- **Why it is designed this way:** [`docs/adr/`](docs/adr/). The main ones are
  [005](docs/adr/005-verbs-are-the-function.md) (a command is its function),
  [006](docs/adr/006-an-argument-is-a-type.md) (an argument is a type),
  [016](docs/adr/016-hicli-spells-one-value-argument-types.md) (`#[arg(…)]`,
  `Choice` and the help rule),
  [018](docs/adr/018-a-spelled-type-parses-itself-and-names-itself.md) (which
  `FromStr` parses a word, and errors that name the argument),
  [014](docs/adr/014-a-lazy-context-needs-no-api.md) (a context that connects on
  first use),
  [011](docs/adr/011-curried-commands-and-default-commands.md) (curried groups and
  default commands) and
  [012](docs/adr/012-command-errors-convert-through-into-failure.md) (errors).

## Development

```sh
devenv shell
just ci      # fmt-check + clippy + test
```

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option.
