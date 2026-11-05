# ADR-015: Coloured output through `print_message`

## Status

Accepted (2026-11-05)

Closes johlrogge/hicli#31 (help and errors from `App::run` are never
coloured). Corrects ADR-013, whose last rejected alternative assumed `run`
already printed colour. ADR-014 is reserved for johlrogge/hicli#7.

## Context

- **`run` printed plain text, always.** It printed a parse failure with
  `eprintln!("{}", failure.unwrap_stderr())` or
  `print!("{}", failure.unwrap_stdout())`, and a command's `Failure` with
  `eprintln!("{}", e.message())`. `unwrap_stderr` and `unwrap_stdout` render
  with `Doc::monochrome`, so the styling `bpaf` records (emphasis, literals,
  metavariables, invalid input) was dropped, and hicli did not turn on
  `bpaf`'s `color` feature in the first place.
- **`bpaf` has one coloured path: `ParseFailure::print_message(max_width)`.**
  It decides colour with `Color::default()` and prints in the same call:
  `println!` for help and version, `print!` for a completion payload, and
  `eprintln!("Error: {render}")` for a usage error, with `Error: ` bold red
  when coloured. The renderer (`Doc::render_console`) and `Color` are
  `pub(crate)`.
- **How `bpaf` decides** (verified against bpaf 0.9.28 and supports-color
  3.0.2). With its `color` feature on, `Color::default()` is `Dull`, or
  `Bright` with `bright-color` unless `dull-color` is also on. It falls back to
  monochrome unless `supports_color::on` answers `Some` for **both** stdout and
  stderr. It is evaluated on every call; nothing is cached. `supports_color`,
  per stream:
  1. `FORCE_COLOR` set to anything but `0` or `false` (empty included) forces
     colour, over everything below. Only when `FORCE_COLOR` is unset,
     `CLICOLOR_FORCE` other than `0` does the same.
  2. Otherwise `NO_COLOR` other than `0` (empty included), `TERM=dumb`, or a
     stream that is not a terminal (unless `IGNORE_IS_TERMINAL` is set and not
     `0`) means no colour.
  3. Otherwise colour if `TERM` is set (and, on Unix, is not `dumb`; on
     Windows an unset `TERM` counts), `COLORTERM` is set, `CLICOLOR` is other
     than `0`, or a CI environment is detected. `CLICOLOR=0` is not an off
     switch on its own.

## Decision

### 1. A default-on `color` feature that forwards `bpaf/color`

```toml
[features]
default = ["color"]
color = ["bpaf/color"]
```

It forwards `bpaf/color`, not `bpaf/dull-color`: `Color::default()` lets
`dull-color` win whenever it is on, so forwarding it would override a
consumer's own `bpaf = { version = "0.9", features = ["bright-color"] }`.
The dull palette is what `color` alone gives. There is no `cfg` in hicli's
code: the feature only changes what `bpaf` does.

### 2. hicli always prints through `print_message`

`report_failure` prints every `ParseFailure` with
`print_message(help::WIDTH)`, where `help::WIDTH` is 100, `bpaf`'s default
`Info::max_width`, which hicli never changes. This is so even with the feature
off, so the text is the same with and without colour. Two output changes come
with it, both accepted:

- a usage error starts with `Error: `;
- help and `--version` are followed by a blank line (`println!` of text that
  already ends in a newline).

### 3. The `did you mean` hint is a `Doc` appended to `bpaf`'s error

`suggested` returns the `ParseFailure` with `help::with_suggestion(&doc, name)`
in place of the error: `bpaf`'s `Doc`, a hard line break (`"\n "`), then
`did you mean `, the name as a literal, and `?`. Monochrome it is byte for
byte the 0.3 string, `"<msg>\ndid you mean `<name>`?"`; coloured, the name is
styled like the rest of the error.

### 4. A command's `Failure` takes the same path

`report_outcome` prints a `Failure` as
`ParseFailure::Stderr(help::verbatim(e.message())).print_message(help::WIDTH)`.
A parse error and a command failure look the same, `Error: <message>`, the
prefix bold red on a terminal. `help::verbatim` keeps the message as written:
each line on its own output line, indents and blank lines kept (a plain `Doc`
would reflow a lone `"\n"` into a space and turn a four-space indent into a
code line). Two differences from `eprintln!` remain: a trailing newline is
dropped, and a line longer than 100 columns wraps at a space, its continuation
at column zero. `src/help.rs` pins both.

### 5. The test seams stay monochrome

`App::parse`, `App::complete` and `App::into_parser` are unchanged: they
return `bpaf`'s `ParseFailure`, which `unwrap_stderr`/`unwrap_stdout` render
monochrome and without the `Error: ` prefix. Snapshot tests are the same in
every environment and with every feature set. `App::run_with_args` prints
exactly what `run` prints, colour included, and returns only the code.

### 6. No `--color` flag

`Color` is crate-private in `bpaf`, so hicli cannot pass a choice to
`print_message`. The environment variables above are the only controls, and
the crate docs list them.

## Alternatives considered

- **Re-render colour in hicli.** Impossible: the renderer and `Color` are
  `pub(crate)`, and a `Doc`'s tokens are private. Reimplementing them would
  duplicate `bpaf`'s styles and its terminal detection.
- **`print_message`, then `eprintln!` the hint.** The hint would be a second,
  unstyled write outside `bpaf`'s render; building it as a `Doc` costs
  nothing extra and keeps the error one message.
- **Default off.** Most consumers would never find the feature, and would
  keep printing plain help on a terminal. Turning it off is one line for those
  who want that.
- **A `cfg(feature = "color")` branch per path** (`print_message` when on, the
  0.3 `eprintln!` when off). Two outputs to test and document, differing by
  more than colour (`Error: `, the blank line), for nothing a consumer asked
  for.
- **Keep `eprintln!` for `Failure`.** Recommended by the architect, overruled
  by the user: a usage error and a command's failure would look different, one
  `Error: `-prefixed and coloured, the other not.

## Consequences

- Output changes, no API breaks: the `Error: ` prefix on usage errors and
  command failures, the blank line after help and version, the styled hint.
  The README's transcripts and the promo site show the prefix.
- `bpaf`'s `color` feature pulls in `supports-color` and `owo-colors` (and
  `is_ci`). `default-features = false` removes them.
- `just ci` runs `cargo check --no-default-features`, so the crate keeps
  building without the feature.
- A command whose message ends in a newline no longer prints a blank line
  after it, and a message line over 100 columns wraps.
