# ADR-013: hicli renders every usage line, and names the binary

## Status

Accepted (2026-11-05)

Corrected (#31), 2026-11-05: in 0.3 `run` printed no coloured output; the
coloured path arrived with ADR-015. The last of the "Alternatives considered"
is reworded accordingly.

Amended (#30), 2026-11-05: §4, long usage lines wrap between items.

Closes johlrogge/hicli#21 (usage lines name no binary) and johlrogge/hicli#28
(a trailing space after a command without arguments). Keeps ADR-009 §3: hicli
still never hands `bpaf` the program name. Generalises what ADR-011 §4 did for
curried commands to every command.

## Context

- **#21.** `playr playback stop --help` printed `Usage: playback stop`. The
  binary's name was missing, unlike the help of nearly every Unix tool.
- **#28.** The same line actually ended in a space, `Usage: playback stop `.
  `bpaf`'s `Doc::write_path` writes "word + space" for every path word, then
  the parser's own part. When a command has no arguments that part is empty,
  and the space is left at the end of the line. Editors and linters that strip
  trailing whitespace broke snapshot tests of such pages.
- **`bpaf` names the program only via `Args::set_name`.** ADR-009 §3 rejected
  that: once it has a name, `bpaf` handles `--bpaf-complete-style-*` itself. It
  `println!`s a script and calls `std::process::exit`, inside hicli's run path.
- **hicli already rendered some usage lines.** Under a curried group, `bpaf`'s
  derived line drops the curried value from the path. So curried commands
  already went through `OptionParser::with_usage`, with hicli spelling the path
  (ADR-011 §4, spike H-S1). That `usage_prefixed` had the same trailing-space
  defect (`format!("Usage: {} ", …)`).

## Decision

### 1. Every usage line goes through `with_usage`

Every mounted command, every group and the top-level parser get
`OptionParser::with_usage(help::usage_line(shown))`. The line is:

- `Usage:`, styled as `bpaf` styles its own (`Usage` emphasised);
- the *shown* path: the binary's name, the command names, and the metavar of
  every curried value on the way down (`playr source SOURCE check-updates`);
- `bpaf`'s own derived part (the parser's items, such as `[--apply]`), only if
  it renders to something.

They are joined by single spaces, and the line never ends in one.

### 2. The binary's name: `App::bin_name`, else `argv[0]`

`App::bin_name(name)` sets the name explicitly. If it is not set, `App::run`
uses the file name of `argv[0]` (`/usr/bin/playr` and `./target/debug/playr`
are both `playr`; an empty `argv[0]` gives no name). The test seams,
`App::parse` and `App::run_with_args`, read no `argv[0]`, so they show no name
unless `bin_name` is set. The name is never part of `Invocation::path`.

### 3. hicli still never calls `Args::set_name`

ADR-009 §3 stands. The name is display text, written into the usage line by
hicli. `bpaf` never sees it, so `--bpaf-complete-style-*` stays an unknown flag
(exit 2) with or without `bin_name`. `tests/usage.rs` pins that.

### 4. Long usage lines wrap between items (amendment, #30)

**The defect.** `mdma rekordbox export --help` printed

```
Usage: mdma rekordbox export [--playlist=PLAYLIST]... [--output=OUTPUT] [--format=FORMAT] [--replace
]
```

The root cause is in `bpaf`'s console renderer (`buffer/console.rs`). It
wraps per chunk, and a chunk ends at a style boundary as well as at a space:
`[` (text), `--replace` (literal) and `]` (text) are three chunks, so the line
breaks between whichever two cross column 100, and a `[` or `]` lands on a
line apart from its option. The continuation margin is zero, so the next line
starts at column zero. (The overflow check also compares a chunk's byte length
with a character position.) `tests/bpaf_contract.rs` (W-S1) pins this with
plain `bpaf`, and an issue is filed upstream.

There is no clean fix in hicli: the derived part reaches `with_usage` as an
opaque `Doc`, whose tokens are private and whose public renderings are
monochrome.

**The workaround**, all in `help::usage_line`:

- **A line that fits in 100 columns is unchanged**: it is built from `bpaf`'s
  own derived tokens, byte for byte as before, styles included. Only a longer
  line takes the path below.
- **Layout.** The derived part's text (unwrapped, monochrome) is split at
  spaces into units that never break: a `...` word stays with the word before
  it (`COMMAND ...`), a `|` word with the word after it
  (`| [--artist=ARTIST]`). Units are filled greedily into lines of at most
  100 columns, measured in bytes as `bpaf`'s check measures them; a unit too
  long for a line gets one of its own.
- **Indent.** Every continuation is indented to where the derived part
  starts, under the first item; if that is past half the width, 7 columns,
  under `Usage: `. Each continuation is a `"\n "` hard break followed by the
  indent as a `Doc` of its own, so `bpaf` never reads the indent as a code
  line.
- **Styling.** Each line is re-styled from its text by `bpaf`'s usage grammar
  (`Doc::write_meta`): `COMMAND ...` and metavars as metavars, an option's
  name as a literal, brackets, parentheses, `|`, `...`, `=` and spaces as
  text.

```
Usage: mdma rekordbox export [--playlist=PLAYLIST]... [--output=OUTPUT] [--format=FORMAT]
                             [--replace]
```

**Known mis-styles**, on wrapped lines only: a metavar that contains a space
or a bracket, and a custom `any` metavar `Doc`, are styled by the grammar
rather than as `bpaf` styled them. The text is always `bpaf`'s.

**Removal.** The workaround is removed once `bpaf` wraps usage lines between
items itself; W-S1 in `tests/bpaf_contract.rs` fails on that upgrade and says
so.

## Why `argv[0]` here, when ADR-009 rejected it

- ADR-009 §4 rejects `argv[0]` as the name **pasted into shell code** that
  registers completion. There the name has to be the command word the shell
  completes against, and a path or wrapper name would be wrong or unsafe.
- A usage line only **displays** text. The name the user actually typed,
  including a symlink's name, is the right thing to show them. `bpaf`'s own
  `run()` does the same, and so do most Unix tools.
- The explicit setter wins when it is set. It also lets a test show exactly the
  line production prints.

## Alternatives considered

- **`Args::set_name`.** Rejected, per ADR-009 §3: it turns on `bpaf`'s
  print-and-exit handling of `--bpaf-complete-style-*`.
- **An explicit `bin_name` only, with no `argv[0]` fallback.** Rejected: every
  consumer would have to opt in to get #21 fixed, and the default would stay
  the broken line.
- **Trim `bpaf`'s trailing space from rendered output.** Rejected: help reaches
  hicli as a `ParseFailure`, and there is nothing public to trim before it is
  rendered. Trimming the rendered string would fix only the monochrome text
  `unwrap_stdout` returns. `bpaf`'s one coloured path,
  `ParseFailure::print_message`, renders and prints in a single call and gives
  hicli no string to trim.

## Consequences

- `Mount::root` takes the binary's name. `Mount::is_shifted` and
  `usage_prefixed` go away: there is one usage renderer, `help::usage_line`.
- Help text changes, but no API breaks. Every `Usage:` line `run` prints now
  starts with the binary's name and none ends in a space. Snapshot tests of
  help pages need re-blessing; with `.bin_name(..)` they match `run`.
- `tests/bpaf_contract.rs` keeps pinning `bpaf`'s raw derived line, trailing
  space included. Re-run it on `bpaf` upgrades: if `bpaf` changes how it
  derives the part handed to `with_usage`, it fails there first.
- ADR-008's deferred "usage-line override" for consumers stays deferred. This
  ADR changes how hicli renders the line, not who controls it.
