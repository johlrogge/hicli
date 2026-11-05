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

> 0.3.0, pre-1.0: a minor version may break things. Every break is listed in
> [`CHANGELOG.md`](CHANGELOG.md). MSRV: Rust 1.99.

## Quick start

Most examples below build one CLI step by step. `playr` controls a networked music
player: it starts and stops playback, searches a library and manages music sources.
First, three commands and a global option:

```toml
[dependencies]
hicli = "0.3"
bpaf = "0.9"   # needed for #[derive(Bpaf)] on your own argument types
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

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Output(
    /// An audio output device
    #[bpaf(positional("NAME"))]
    pub Name,
);

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
  you write `#[command(name = "…")]`. The first line of the doc comment is its help.
  The function itself is unchanged: `playback::stop(&ctx)` still works.
- **`#[group]` on a module makes it a group.** Its commands are the `#[command]`
  functions and `#[group]` modules inside it, in source order. There is no list to
  keep in sync. A command or group without a doc comment is a compile error.
- **An argument is a type.** `set_output` takes an `Output`, a newtype that derives
  `Bpaf` (how it is spelled) and `Arg` (so hicli can use it). Define it once and use
  it in every command that needs it. hicli has no `Arg` impls for `String`, `u64` and
  so on, so you always use a newtype
  ([ADR-006](docs/adr/006-an-argument-is-a-type.md)). Inside it is the value's own
  type, here a `Name`.
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
  configuration in it. Read secrets from the environment in that closure.
- **Exit codes:** `0` on success, `1` for a runtime failure, `2` for a usage error.
  `run` returns the `ExitCode`. It never calls `process::exit`.

### Bad values never reach a command

The newtype says how an argument is spelled. The type inside it says what a valid
value is. bpaf parses the value with that type's `FromStr`, so its rules live in one
place, and every command receives a value that is already valid:

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
`:port`. A value that doesn't parse is a usage error, and no command runs:

```console
$ playr playback set-output HDMI
Error: couldn't parse `HDMI`: a name uses only a-z, 0-9 and -
$ echo $?
2
$ playr --node pi.local:http ping
Error: couldn't parse `pi.local:http`: `http` is not a port number
```

Known gap: the message names the value and the reason, not the option or positional
it was given to. With several values of the same type, the user has to work out
which one failed ([#26](https://github.com/johlrogge/hicli/issues/26)).

A value with no rules, like the free-text search query below, can stay a `String`
inside its newtype. Paths are `PathBuf`, and choices are enums (see
[Choice values](#choice-values)).

## Growing the example

The following sections add to `playr`. The complete program, with tests, is
[`examples/playr.rs`](examples/playr.rs).

### Options, switches and positionals

`playr search` filters the library. Several options can share one `Bpaf` struct.
Each field's doc comment is its help line. `Bpm` accepts 20 to 300, written like
`Name`:

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

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Query(
    /// Free-text query over title, artist and album
    #[bpaf(positional("QUERY"), optional)]
    pub Option<String>,
);
```

A command's parameters are parsed in the order they are declared. **Declare the
types that contain positionals last.** bpaf requires this, and a debug build checks it
the first time it runs. A release build doesn't check it up front: the same mistake
panics when `--help` is rendered. The rule is only about parameter order: on the
command line, flags can go anywhere.

Put a doc comment on the field, not on the newtype. bpaf turns a doc comment on a
struct into a section heading (that is how `Global options` appears above). On a
one-field newtype, that would print a heading with nothing under it.

### Values that start with `-`

`playr search --added=-7` means "added in the last 7 days". Write such a value glued
to its option with `=`. A plain `argument` reads it, and the type inside decides what
is valid: `DaysBack` accepts `-7` and rejects `7`, `-0` and `-x`.

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Added(
    /// Only tracks added in the last DAYS days, e.g. -7
    #[bpaf(long("added"), argument("DAYS"), optional)]
    pub Option<DaysBack>,
);
```

As two words, `-7` reads as a flag, and bpaf says how to write it instead:

```console
$ playr search --added=-7
localhost: search None artist=None bpm=None unplayed=false added=Some(DaysBack(7))
$ playr search --added -7
Error: `--added` requires an argument `DAYS`, got a flag `-7`, try `--added=-7` to use it as an argument
$ playr search --added=-x
Error: couldn't parse `-x`: `-x` is not a number of days back, such as -7
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

Limits: one default per group; none at the top level or in an `args`/`curried`
group; a default takes no `name`, `alias` or `hide`, and its doc comment is one
line (it has no help page of its own). That line titles the default's options in
the group's `--help`, as with `search` above; a default without options, like
`list` here, has its line shown under `Usage:`, and the usage reads
`[COMMAND ...]` because naming a command is optional. See
[ADR-011](docs/adr/011-curried-commands-and-default-commands.md).

### Object, then verb: `source <name> check-updates`

`playr source list` lists the sources, and `playr source bandcamp check-updates`
acts on one of them. Here a value (the source name) comes between the group and
the command. Use `#[group(args = T)]` for the group and `#[command(curried)]` for
each command that receives the value, which must be its first parameter:

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_sources)]
pub struct SourceName(
    /// A source on the node: bandcamp, beatport or upload
    #[bpaf(positional("SOURCE"))]
    pub Name,
);

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
nothing typed also offers the curried command names, which are not valid until a
source is typed. For the others, such as `source NAME --help` showing the group's
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

### Choice values

`playr sort bpm` takes one of a fixed set of words. For now, write an enum with
`FromStr`. Its error message is shown to the user:

```rust
// A choice value: an ordinary enum with `FromStr`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortKey {
    Bpm,
    Title,
    Artist,
}

impl FromStr for SortKey {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "bpm" => Ok(Self::Bpm),
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            other => Err(format!("`{other}` is not one of: bpm, title, artist")),
        }
    }
}

#[derive(Debug, Clone, Bpaf, Arg)]
pub struct SortBy(
    /// What to sort by: bpm, title or artist
    #[bpaf(positional("KEY"))]
    pub SortKey,
);
```

```console
$ playr sort key
Error: couldn't parse `key`: `key` is not one of: bpm, title, artist
```

Shell completion doesn't work for these values yet: `sort b<TAB>` offers nothing.
Built-in choice enums are tracked in [#8](https://github.com/johlrogge/hicli/issues/8).

### Help text

All help comes from doc comments. A command's first doc line is its summary, the
following paragraphs are its description, and an `# Examples` section is printed at
the end of its `--help`:

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
fn sort(desc: Descending, by: SortBy) {
    println!("sorting by {:?}, descending: {}", by.0, desc.0);
}
```

```console
$ playr sort --help
Sort track ids read from stdin

Usage: playr sort [-d] KEY

Reads one track id per line and prints them sorted.

Available positional items:
    KEY               What to sort by: bpm, title or artist

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

### Colour

On a terminal, `run` prints help and errors in colour: headings and option names in
bold, metavariables underlined, and `Error: ` in bold red. It needs both stdout and
stderr to be terminals, so `playr --help | less` and `playr --help 2>/dev/null` print
plain text. `NO_COLOR=1` turns colour off and `FORCE_COLOR=1` forces it on; there is
no `--color` flag. To never print colour, depend on
`hicli = { version = "0.4", default-features = false }`. For bright colours, add
`bpaf = { version = "0.9", features = ["bright-color"] }`. `App::parse` and the
snapshot tests built on it always get plain text without the `Error: ` prefix. See
[ADR-015](docs/adr/015-coloured-output-through-print-message.md).

### Shell completion

Completion is attached to the argument type, so every command that takes a
`SourceName` completes source names:

```rust
/// Runs on every TAB: never fails, and filters by what was typed.
fn complete_sources(typed: &SourceName) -> Vec<Candidate> {
    ["bandcamp", "beatport", "upload"]
        .into_iter()
        .filter(|s| s.starts_with(&typed.0.to_string()))
        .map(|s| (s.to_string(), None))
        .collect()
}
```

`#[arg(complete = complete_sources)]` on the type (shown above) connects the two. For
paths, use `#[arg(complete = files)]` or `#[arg(complete = dirs)]`, which let the
shell complete them. **Your completer must filter by the typed prefix itself.** bpaf
passes its results to the shell unchanged.

Command names complete automatically. To install completion, render the script from a
hidden command:

```rust
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct ShellArg(
    /// A shell to print completions for: bash, zsh or fish
    #[bpaf(positional("SHELL"))]
    pub hicli::Shell,
);

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
        set_output_needs_a_name: { playback "set-output" } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("NAME"));
        },
        output_names_are_checked: { playback "set-output" HDMI } => |r| {
            let e = r.err().expect("a usage error");
            assert_eq!(exit_code_for(&e), 2);
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
        };
        assert!(upload(AudioFile("/no/such/file.flac".into()), &ctx).is_err());
    }
}
```

- `cli! { … }` turns the words you would type into an argv, and `cli_test!` writes
  one `#[test]` per line. **Quote words with hyphens** (`"set-output"`, `"--dry-run"`).
  Without quotes, Rust splits `set-output` into three tokens.
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
2. **Add `bpaf = "0.9"` to your own dependencies.** `#[derive(Bpaf)]` generates
   `::bpaf::` paths. Cargo still builds a single copy of bpaf.
3. **Declare positional parameters last** (see above).
4. **There are no `Arg` impls for primitives.** Give each value a newtype, with the
   value's own type inside.
5. **Completers filter their own candidates, and they only see values that parsed.** A
   `String` completes from the first keystroke. A value checked by `FromStr`, such
   as a `Name`, a number or a choice, doesn't complete until the partial word parses.
   Choose rules that every prefix of a valid value also passes: `b` is a valid
   `Name`, so `source b<TAB>` offers `bandcamp` and `beatport`. An empty word is not
   a `Name`, so `source <TAB>` offers only the commands.
6. **`#[arg(complete = …)]` goes only on a single-field type.** On a multi-field
   struct or an enum it is a compile error.
7. **`Globals` doesn't derive `Arg`.** It is parsed once at the top level.
8. **A `#[group] pub mod x;` that lives in its own file must be declared at the top
   level of a file**, not inside an inline module. hicli puts `#[path = "x.rs"]` on
   it, and rustc then looks up a plain `mod helper;` inside `x.rs` next to `x.rs`,
   not under `x/` (E0583). Until that is fixed, write
   `#[path = "x/helper.rs"] mod helper;`
   ([#25](https://github.com/johlrogge/hicli/issues/25)). A nested `#[group] mod` is not
   affected.
9. **The attribute keys:**
   - `#[command(name = "…", alias = "…", hide)]`, plus `default` and `curried` (above).
   - `#[group(alias = "…", hide)]`, plus `args` and `curried` (above). A group has no
     `name`: it is the module's.
   - `#[group(ctx = T)]` names a group's context. Without it, the group takes the
     first `&T` among its commands, and a command with no `&T` is generic over `Ctx`.
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
  and can't be overridden per subcommand. Argument spelling uses bpaf's attributes.
  `--help` exits 0 and usage errors exit 2.
- **Why it is designed this way:** [`docs/adr/`](docs/adr/). The main ones are
  [005](docs/adr/005-verbs-are-the-function.md) (a command is its function),
  [006](docs/adr/006-an-argument-is-a-type.md) (an argument is a type),
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
