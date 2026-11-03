# hicli

**hi cli — adds the line to your command.** CLI-enable your code: a modular,
publishable subcommand framework for Rust, built on
[`bpaf`](https://docs.rs/bpaf).

Say it out loud (*hej cli*, if you are Swedish about it), or read it as *hi-fi →
hi-cli*: a high-fidelity command-line interface rendered from the code you
already have.

> **Status:** v1 is implemented and covered by an in-process test suite. The source is
> public at [github.com/johlrogge/hicli](https://github.com/johlrogge/hicli). 0.1.0 is
> the first release, published to crates.io as `hicli` and `hicli-macros`. Before
> 1.0, a minor version may carry breaking changes.

## The idea

Most `clap`-based CLIs end up less modular than they could be: one subcommand should
be one self-contained unit — its arguments and its behavior — but in practice you get
a derive-enum plus a hand-written central `match` that grows without bound.

`hicli` makes the alternative the path of least resistance:

- **A subcommand is a documented function.** `#[command]` leaves that function exactly as
  you wrote it — same name, same signature, same visibility — and *adds* a hidden
  registration handle beside it.
- **A group of subcommands is a documented module.** `#[group]` leaves the module
  alone too, and discovers its children: a child is a child because it is *in* the
  module, in source order. There is no list to keep in step with the code.
- **A group whose body is a file** is the same `#[group]` on `pub mod wallet;` — one
  file per group.

There is no central dispatcher, and no name, help string or registration line written
twice.

See [`docs/adr/005-verbs-are-the-function.md`](docs/adr/005-verbs-are-the-function.md)
for why it looks like this, [`docs/adr/006-an-argument-is-a-type.md`](docs/adr/006-an-argument-is-a-type.md)
for why an argument is a type with completion attached, and [`docs/PLAN.md`](docs/PLAN.md)
for the full API and the `bpaf` capability analysis that grounds it.

## A whole CLI

```toml
# Cargo.toml
[dependencies]
hicli = "0.1"
bpaf = "0.9"    # for `#[derive(Bpaf)]` on your own argument structs — see "Rules" below
```

```rust
// src/main.rs
use std::process::ExitCode;

use bpaf::Bpaf;
// One `use` for `command`, one for `group` (and, below, `Arg`): the
// attribute/trait and the function/derive they expand into share a name across
// two namespaces each.
use hicli::{app, command, group, App, Arg, Failure, Outcome};

/// Global options, accepted before or after the subcommand.
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// Base directory for the app's data.
    #[bpaf(long, argument("PATH"), fallback(".".into()))]
    home: String,
}

/// The shared context, built once after parsing. Anything secret is read from the
/// environment here — never as a parser field, where `--help` would advertise it.
pub struct Ctx {
    pub home: String,
}

fn mk_ctx(g: Globals) -> Result<Ctx, Failure> {
    Ok(Ctx { home: g.home })
}

// An argument is a *type* (see docs/adr/006-an-argument-is-a-type.md), written
// once and taken by every command that needs it. `#[derive(Bpaf)]` names the parser
// after the struct: `EventDir` -> `event_dir()`. `#[derive(Arg)]` turns that into
// what `#[command]` actually calls, and `#[arg(complete = dirs)]` hands completion
// to the shell itself — no candidate list to maintain.
/// Where the event lives.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]
pub struct EventDir(#[bpaf(positional("DIR"))] pub String);

/// The event's display name.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct EventName(#[bpaf(positional("NAME"), some("a name is required"))] pub Vec<String>);

// The newtype the completion story is *for*: every command that takes an `Id`
// inherits its completion, declared here once.
/// An identity in this home.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_ids)]
pub struct Id(#[bpaf(positional("ID"))] pub String);

/// The ids under `$APP_HOME`, filtered by what has been typed.
///
/// Completion runs in the user's shell on every TAB, so this reads the
/// environment (never `--home`: bpaf hands a completer only the parsed value),
/// never panics, and answers an unreadable home with an empty list rather than
/// an error. `bpaf` passes the result through unfiltered, so the prefix filter
/// — `starts_with` — is ours.
fn complete_ids(typed: &Id) -> Vec<hicli::Candidate> {
    let Some(home) = std::env::var_os("APP_HOME") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(std::path::Path::new(&home).join("ids")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.starts_with(&typed.0))
        .map(|name| (name, None))
        .collect()
}

// A group is a documented module. Its children are `create`, `list` and the
// nested `audience` group — in that order — because they are in it.
/// events and instances
///
/// An event is a directory; an instance is a branch of it.
#[group]
mod event {
    use super::*;

    /// create an event
    ///
    /// The event directory is created if it does not exist.
    #[command]
    pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
        println!("[{}] created {} in {}", ctx.home, name.0.join(" "), dir.0);
        Ok(())
    }

    /// list the events in a directory
    #[command]
    pub fn list(dir: EventDir, ctx: &Ctx) -> Outcome {
        println!("[{}] nothing in {} yet", ctx.home, dir.0);
        Ok(())
    }

    // A nested group is a nested module, marked the same way. Its context is not
    // written anywhere either: `define` takes `&Ctx`, so the whole tree does.
    /// manage audiences
    #[group]
    pub mod audience {
        use super::*;

        /// define an audience
        #[command]
        pub fn define(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
            println!("[{}] audience {} in {}", ctx.home, name.0.join(" "), dir.0);
            Ok(())
        }
    }
}

// A group whose body is a file — `src/wallet.rs`, found by Rust's own module
// rules. `#[group]` reads that file to discover the group's children, exactly
// as it reads an inline module body.
/// cards and deliverables
#[group]
pub mod wallet;

/// show status
#[command]
fn status(ctx: &Ctx) {
    println!("[{}] ok", ctx.home);
}

/// show an identity
///
/// `id: Id` is the entire completion story for this command: it declares nothing
/// and knows nothing about `$APP_HOME` — it takes an `Id`, and an `Id` completes.
#[command]
fn show(id: Id, ctx: &Ctx) {
    println!("[{}] {}", ctx.home, id.0);
}

/// The whole command tree. Named, so the tests below drive the same one.
///
/// Each bare name is a registration: `commands!` reads it as the hidden handle
/// `#[command]` or `#[group]` wrote beside the item of that name. A
/// group and a command are indistinguishable here, which is the point — `event` is
/// a module, `status` is a function, both are just names in a list.
fn app() -> App<Globals, Ctx> {
    app! {
        globals: globals(),
        commands: [event, wallet, status, show],
    }
}

fn main() -> ExitCode {
    // `run` *returns* the exit code — hicli never calls `std::process::exit`.
    app().run(mk_ctx)
}
```

```rust
// src/wallet.rs — the body of the `wallet` group. Ordinary Rust: the group's
// help lives on the declaration, so what is left here is the commands.
use hicli::{command, Outcome};

use crate::Ctx;

/// list the cards in your wallet
#[command]
pub fn cards(ctx: &Ctx) -> Outcome {
    println!("[{}] no cards yet", ctx.home);
    Ok(())
}
```

That is the whole thing: `app event create ./ev "Summer Party"`, `app wallet cards`,
`app --help`, `app event create --help`, typo suggestions and exit codes included.

- **The name** is the ident, `_` → `-` (`add_admin` → `add-admin`); a command can
  override it with `#[command(name = "…")]`, a group cannot.
- **Another spelling, or none at all,** comes from the same two keys on both
  attributes: `#[command(alias = "ls")]` / `#[group(alias = "…")]` (repeatable) adds an
  unlisted alias — the canonical path never changes — and `#[command(hide)]` /
  `#[group(hide)]` drops a command or group out of `--help`, completion and
  suggestions while it still runs when typed exactly.
- **The help** is the doc comment — first line for the one-line listing and the
  `--help` heading, the rest for the long description, and a trailing `# Examples`
  section (fence it ```` ```sh ````) rendered verbatim after the options however it is
  headed. **A `#[command]` or `#[group]` without a doc comment is a compile error.**
- **The arguments** are the command's owned parameters, each a type implementing
  `Arg` — ordinarily `#[derive(Bpaf, Arg)]`. `EventDir` above is shared by three
  commands: an argument is a type, written once, helped once, completed once,
  tested once. `#[command]` has no parameter attributes and never sees an
  argument's spelling — it calls `<T as Arg>::parser()`. An optional
  `#[arg(complete = …)]` attaches completion to the type, so `id show` (and
  every future command that takes an `Id`) inherits it for free. See
  [`docs/adr/006-an-argument-is-a-type.md`](docs/adr/006-an-argument-is-a-type.md).
- **The context** is the single `&T` parameter; `T` fixes the command's `Ctx`. A
  command with no `&T` parameter is generic over `Ctx`. A group takes its `Ctx` from
  the first `&T` among its children, or from `#[group(ctx = T)]` when you say so.
- **The children** of a group are every `#[command]` function and every
  `#[group]` module (inline or a file) in the body, in source order. Nothing else is — not
  an argument struct, not an `impl`, not an unmarked `mod`, not `#[cfg(test)] mod
  tests`.
- **The return** is anything implementing `IntoOutcome`: `()`, `Success`,
  `Outcome` (`= Result<(), Failure>`) or `Outcome<Success>`. Usage failures
  exit 2, runtime failures 1, a `Success` carries its own code and prints nothing.

The app itself, not a command, carries two more things, set on the composition root:
`App::about(..)` is the text printed above `Usage:`, and
`App::version(env!("CARGO_PKG_VERSION"))` wires `--version`/`-V` to print it and exit
0 (`app! { … }.about(..).version(..)` reads fine). Installable completion is the same
idea one level up: `hicli::completion_script(Shell, bin) -> String` renders the
bash/zsh/fish script for a binary name, which you wire to a hidden command of your
own. See
[`docs/adr/008-command-metadata-and-help-layout.md`](docs/adr/008-command-metadata-and-help-layout.md)
and
[`docs/adr/009-completion-scripts-are-rendered-by-hicli.md`](docs/adr/009-completion-scripts-are-rendered-by-hicli.md).

One word, four things, in three namespaces: `Command<Ctx>` (the tree node),
`hicli::command` (the function that builds a leaf), `#[command]` (the attribute)
and `commands!` (the list). They do not collide, and **one
`use hicli::command;` imports both the attribute and the function** — the same
trick `#[group]` and `hicli::group` already rely on.

`#[command]` expands to your function, untouched, plus one hidden sibling —
`pub fn __hicli_create() -> Command<Ctx>`, the *handle*. `#[group]` expands to your
module, untouched, plus `__hicli_event()` beside it, which calls the handle of each
child one level down. Both are plain Rust calling public API (`hicli::command`,
`hicli::group`, `App::new(…).commands(…)`), so a command or a group that outgrows
the macro can be written out by hand and nothing is lost.

Two runnable versions of the above — a nested group, a `guard`, an external subcommand
— are [`examples/demo.rs`](examples/demo.rs) (groups inline) and
[`examples/files/`](examples/files/) (groups in files); try `just demo --help` and
`just demo-files --help`.

## How to test it

Testing is in-process and first-class: no subprocess, no `assert_cmd`, no golden
files. There are five layers, and each one is reachable on its own.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hicli::{cli, cli_test, exit_code_for};

    // 1. Argument level: an argument struct is an ordinary bpaf parser, tested
    //    once and trusted by every command that takes it.
    #[test]
    fn an_event_name_takes_many_words() {
        use bpaf::Parser;
        let n = event_name()
            .to_options()
            .run_inner(&["Summer", "Party"][..])
            .expect("parses");
        assert_eq!(n.0, ["Summer", "Party"]);
    }

    // 2. Function level: the command *is* the function, called exactly as it is
    //    declared. No handle, no `run`, no parser.
    #[test]
    fn create_writes_the_event() {
        let ctx = Ctx { home: ".".into() };
        event::create(
            EventDir("./ev".into()),
            EventName(vec!["Summer".into(), "Party".into()]),
            &ctx,
        )
        .expect("creates an event");
    }

    // 3. Parse level: which command was selected, with which globals.
    //    `cli! { … }` takes the tokens you would type; `cli_test!` writes one
    //    `#[test]` per line, against a fresh `app()` each time.
    cli_test! {
        app: app();

        create_ok: { event create "./ev" "Summer Party" } => |r| {
            let (g, invocation) = r.expect("parses");
            // `path()` says *which* `create` — `event create`, not another group's.
            assert_eq!(invocation.path(), ["event", "create"]);
            assert_eq!(g.home, ".");
        },
        create_needs_a_name: { event create "./ev" } => |r| {
            let e = r.err().expect("expected a parse failure");
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("a name is required"));
        },
        nested_group: { event audience define "./ev" Friends } => |r| {
            let (_g, invocation) = r.expect("parses");
            assert_eq!(invocation.path(), ["event", "audience", "define"]);
        },
    }

    // 4. Run level: parse, `mk_ctx`, the command and the exit-code remap — as a
    //    value.
    //    `run_with_args` returns the `u8` that `main` would have exited with.
    #[test]
    fn a_typo_is_a_usage_error() {
        assert_eq!(app().run_with_args(cli! { stauts }, mk_ctx), 2);
    }

    // 5. Completion level: in-process, no shell. The candidates come from the
    //    *argument type*, not from anything written at the command taking it.
    #[test]
    fn show_completes_ids_from_the_home_directory() {
        std::env::set_var("APP_HOME", fixture_home_with_ids(&["alice", "alfred", "bo"]));
        assert_eq!(app().complete(cli! { show al }), ["alice", "alfred"]);
    }
}
```

`App::parse` / `App::into_parser` drive the parser with no process side effects, and
`App::run_with_args` drives the whole run path and *returns* the code instead of
touching the process — so `main` itself is covered by ordinary tests. `App::complete`
is the same idea for completion: it drives `bpaf`'s own completion wire protocol
in-process, pinned at revision 1 (revision 2 asked for through `run_inner` terminates
the process — a `bpaf` hazard outside hicli's control, so the seam never asks for
it), and returns the candidate values a shell would have offered.

## Rules and papercuts

Thirteen things that will otherwise bite you, stated plainly:

1. **Your crate needs its own `bpaf` dependency** — `bpaf = "0.9"`, no features.
   `#[derive(Bpaf)]` expands to `::bpaf::…` paths, which only resolve against your
   crate's own extern prelude; `use hicli::bpaf;` does not help. Cargo unifies the
   two dependencies, so there is still exactly one `bpaf` in the build, and hicli's
   `pub use bpaf` still gives you the types.

2. **Positional argument structs come last in a command's parameter list.** The
   parameters are composed with `bpaf`'s `construct!` in declaration order, and `bpaf`
   requires every positional to sit at the right-hand end. Declare switches and options
   first, positionals last. It is a rule about the *parameter* order only — a `--flag`
   is still accepted anywhere on the command line.

3. **A bare path in `commands!` is always a handle**, never a value. `commands![event]`
   means `__hicli_event()`; `commands![other::cli::event]` means
   `other::cli::__hicli_event()`. A local that already holds a `Command` cannot be
   written bare — parenthesise it, `commands![(c)]`, or skip the macro and write
   `App::commands(vec![c])`. `app!`'s list follows the same rule, and a macro of your
   own that forwards into `commands!` must forward **token trees** (`$($c:tt)*`): an
   `:expr` capture wraps each element in an invisible group and says "expression" about
   something whose whole meaning is that it is a *name*.

4. **A `#[group]` file module must be at a file's top level.** Rust resolves `mod x;`
   inside an inline `mod a { … }` against `a/`, and a proc macro cannot see the inline
   module path it sits in; nested use is a compile error. Inside an inline module,
   write `#[group]` on an inline module instead.

5. **An external subcommand inside a group needs a hand-written handle.** Discovery
   finds *items*, and an external is a value — there is nothing to mark. Write the
   group's handle yourself, by the documented convention, and `commands![plugins]`
   finds it exactly as if `#[group]` had written it:

   ```rust
   use hicli::{command, commands, Command, ExternalCommand};

   /// print the version
   #[command]
   pub fn version(ctx: &Ctx) {
       println!("[{}] 0.1.0", ctx.home);
   }

   /// A never-executed external subcommand, registered by absolute path.
   fn fmt_tool() -> ExternalCommand {
       ExternalCommand::new("fmt", "format things", "/opt/bin/app-fmt")
   }

   /// `hicli` registration handle for the hand-written `plugins` group.
   #[doc(hidden)]
   pub fn __hicli_plugins() -> Command<Ctx> {
       hicli::group(
           "plugins",
           "the installed plugins",
           commands![version, hicli::external(fmt_tool())],
       )
   }
   ```

   Then `commands: [event, plugins]`. The `__hicli_<ident>` name is documented
   public API for exactly this. (At the *top* level nothing is needed —
   `commands![…, hicli::external(fmt_tool())]` is an expression, and expressions are
   allowed there.)

6. **A group's child order is its source order.** Moving a command in `--help` means
   moving a function. There is no list, so there is nothing else that could say
   otherwise — and a command that was written can never be a command that was never
   registered.

7. **The MSRV is Rust 1.99.** `#[group] pub mod x;` needs attribute macros on
   file-module declarations, stable since 1.99; both crates carry
   `rust-version = "1.99"`, and older toolchains are not supported.

8. **hicli ships no `Arg` impls.** Not for `String`, not for `u64`, not for
   `PathBuf`. A primitive in a command's parameter list is a compile error by design —
   see rule 9 below and
   [`docs/adr/006-an-argument-is-a-type.md`](docs/adr/006-an-argument-is-a-type.md) — give
   the value a newtype and derive `Bpaf, Arg` on it instead.

9. **The completer filters; `bpaf` does not.** `bpaf` passes a completer's output
   through unfiltered — it prefix-filters only the flag names it derives itself. Every
   `#[arg(complete = f)]` function must filter by the partial word itself
   (`.filter(|c| c.starts_with(typed))`), or it offers every candidate at every
   keystroke.

10. **Completion completes what already parses.** `bpaf` hands a completer the
    **parsed** value, not the raw word, so it only fires when the type parses from a
    partial: a `String` (or a `Vec<String>` under `some(..)`) parses the empty word, so
    completion works from the first keystroke; a `u64`, or anything else validated
    through `.parse(..)`, does not — the empty word never reaches the completer, and
    the first TAB yields nothing. When that matters, hand-write `Arg::parser()`,
    complete at the *string* level, and validate after.

11. **`#[arg(complete = …)]` only goes on a single-field type.** A completer on a
    multi-field struct or an enum would be handed a fully built value with fallbacks
    for the other fields, could not tell which field is mid-word, and would never fire
    while another field was being typed — it is a compile error, naming both right
    answers: make the value a newtype, or complete the field itself with `bpaf`'s own
    `#[bpaf(complete(f))]`. A composite that wants its parts' completion hand-writes
    `Arg::parser()`, composing `<Part as Arg>::parser()` with `construct!`.

12. **`Globals` is not an `Arg`, and must not derive it.** Globals are composed at the
    top level by `App::new`, never as a command's parameter, so a `Globals` type never
    goes through `Arg::parser` — deriving `Arg` on it would compile and never run.

13. **An option value that starts with `-` (`--since -7`) needs the `adjacent` + `any`
    pattern, not a plain `argument`.** `bpaf` tokenises `-7` as the short flag `-7`
    before any parser runs, so a plain `argument` accepts `--since=-7` but rejects the
    separate-word spelling. The recipe — the flag and an `any` item glued together with
    `adjacent`, with the separate-word branch `hide()`n so `--help` still shows one
    `--since=DAYS` line — is written out on `Arg`'s own docs (`src/arg.rs`) and pinned
    by `tests/hyphen_values.rs`.

A missing `#[derive(Bpaf)]` on a type named `Id` produces one extra, unrelated-looking
line of rustc noise: it is also the name of `std::process::id`, so the "cannot find
function `id`" error comes with a "consider importing `std::process::id`" suggestion
that is never the fix. Ignore it and add the derive.

## Why `bpaf`

`hicli` is built on and **re-exports** [`bpaf`](https://docs.rs/bpaf) (parser-as-value
combinators). `bpaf`'s parsers are plain values with no process side effects, which is
what makes first-class in-process testing possible. hicli states the `bpaf` version
it needs; cargo unifies it with the one you declare (see rule 1 above).

## Design pillars

- **First-class in-process testability** — see "How to test it".
- **Git-style multi-binary subcommands** with shared help and completion — an
  external binary can be a subcommand (`app foo` execs the binary registered for `foo`,
  by absolute path; the git-style `app-foo` naming is a convention, not the contract)
  while `app --help` and completion still describe it, without ever executing it to
  find out. Because the manifest is the single source for both the parser and the help
  text, the test suite proves "nothing is spawned" by registering an external at a path
  that cannot exist: help, completion and dispatch all still work. The child's exit
  code comes back as a value (`Success::code(n)`), so `hicli` never exits the process
  behind your `main`'s back; on Unix a signal death is reported as `128 + N`, like a
  shell.
- **Typo / "did you mean?" suggestions.** `bpaf` supplies suggestions for misspelled
  commands and flags; `hicli` adds a Levenshtein gap-closer over its top-level
  command registry for the case where `bpaf` stays silent.
- **Trust-conscious external-subcommand discovery** — no implicit `$PATH` scanning,
  ever; external subcommands are allowlisted by absolute path.

## Known deltas from clap

1. **Help text is not byte-identical.** `hicli` renders `bpaf`'s layout, not `clap`'s.
   The target is semantic equivalence — every command and option documented with the
   same information. If you are migrating a `clap` CLI, assert on substrings of help
   output, not on whole snapshots.
2. **Globals live at the top level, not per subcommand.** `hicli` composes
   `construct!(globals, tree)`, so a global flag is accepted both before and after the
   subcommand: `app --home x status`, `app status --home x` and even
   `app event create dir NAME --home x` all parse with `--home` reaching the globals
   (`bpaf`'s positional parsers stop at a dashed token, so a free-form positional does
   not swallow it). The real difference from `clap`'s `global = true` is structural:
   there is no per-subcommand redefinition or propagation — globals are parsed once and
   handed to `mk_ctx`; a subcommand cannot declare, shadow or override them. The one
   exception is an **external** subcommand: everything after its name is forwarded to
   the child verbatim, so a global must *precede* the external's name (`app --home x foo`,
   never `app foo --home x`).
3. **A `--` after an external's name is consumed by `bpaf`** and not forwarded to the
   child. If the child needs a literal `--` in its argv, it has to be passed some other
   way.
4. **There is no argument attribute dialect for spelling.** `clap`'s
   `#[arg(long, short, help = …)]` has no hicli equivalent: arguments are
   `#[derive(Bpaf)]` structs and the spelling attributes are `bpaf`'s, documented by
   `bpaf`. `#[command]` reads only the function's ident, its doc comment and its
   parameter *types*. The one hicli-owned argument attribute, `#[arg(complete = …)]`
   (ADR-006), goes on the type beside `#[derive(Bpaf)]` and says only how it completes
   — never how it parses.
5. **Exit codes are values, not `process::exit`.** `App::run` returns
   `std::process::ExitCode`, so destructors run and `main` stays in control. Usage
   errors are remapped to 2 (`bpaf` uses 1) and `--help` to 0.

## Who uses this

The author's own CLIs, in order: [moniker](https://gitlab.johlrogge.rs/) first (its
handlers are already parser-free), then `mdma-cli`, then `ctx`.

## Getting started (development)

This project runs in an immutable Nix/devenv environment. See `devenv.nix` for the
full toolchain (stable Rust, pinned).

```sh
devenv shell        # or: direnv allow, if you use direnv
just --list          # see all available tasks
just ci               # fmt-check + clippy + test — what CI runs
just demo --help      # run the example CLI (groups inline)
just demo-files --help  # the same CLI with its groups in files
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
