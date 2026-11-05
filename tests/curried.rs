//! Curried commands (ADR-011, johlrogge/hicli#1), through the macro surface:
//! `#[group(args = T)]`, `#[command(curried)]` and `#[group(curried = T)]`.
//!
//! The acceptance spec is mdma-cli's `mdma source <SOURCE-NAME> <COMMAND>`,
//! which clap could only express as an `external_subcommand` re-parsed by
//! `parse_source_args`. The fixture below mirrors mdma's `SourceCommands`
//! command for command — same names, same arguments, same help lines — and the
//! tests port mdma's `parse_source_args` tests (`every_per_source_command_…`,
//! `import_playlist_defaults_…`, `source_list_…`, `unknown_per_source_…`,
//! `source_name_without_command_fails`, `old_name_last_order_is_rejected`,
//! the two help tests), then pin what hicli adds: the value reaching the
//! function, `Invocation::path`, help sections and completion.
//!
//! Everything is in-process: `App::parse`, `App::run_with_args` and
//! `App::complete`, driven by `cli!` or an arg slice.

use std::cell::RefCell;
use std::rc::Rc;

use hicli::bpaf::{pure, Bpaf, ParseFailure};
use hicli::{app, cli, command, exit_code_for, group, App, Arg, Candidate, Failure, Outcome};

// --- the consumer's context and argument types --------------------------------

/// What one command run received: the source (`None` for the fixed `list`),
/// the command, and its other arguments rendered as strings.
type Ran = (Option<String>, &'static str, Vec<String>);

/// The shared context. Every command records what it received.
pub struct Ctx {
    ran: Rc<RefCell<Vec<Ran>>>,
}

impl Ctx {
    fn new() -> Self {
        Ctx {
            ran: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Record a per-source command.
    fn record(&self, source: &SourceName, command: &'static str, args: Vec<String>) -> Outcome {
        self.ran
            .borrow_mut()
            .push((Some(source.0.clone()), command, args));
        Ok(())
    }
}

/// A music source's name, completed from a fixed list — the stand-in for
/// mdma asking the library which sources it has.
#[derive(Debug, Clone, PartialEq, Bpaf, Arg)]
#[arg(complete = complete_sources)]
pub struct SourceName(
    /// the music source
    #[bpaf(positional("SOURCE-NAME"))]
    String,
);

/// The sources on offer, filtered by what has been typed.
fn complete_sources(typed: &SourceName) -> Vec<Candidate> {
    ["bandcamp", "beatport", "discogs"]
        .iter()
        .filter(|c| c.starts_with(typed.0.as_str()))
        .map(|c| ((*c).to_string(), None))
        .collect()
}

/// Download ID to cancel (mdma: `Cancel { id }`).
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct DownloadId(
    /// Download ID
    #[bpaf(positional("ID"))]
    String,
);

/// Source-specific item identifier (mdma: `Resync { identifier }`,
/// `CheckItem { identifier }`).
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Ident(
    /// Source-specific item identifier (e.g. "p123456" for bandcamp)
    #[bpaf(positional("IDENTIFIER"))]
    String,
);

/// mdma: `CheckUpdates { #[arg(long)] apply: bool }`.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Apply {
    /// Auto-apply resync for any stale items found
    #[bpaf(long, switch)]
    apply: bool,
}

/// mdma: `ImportPlaylist { playlist, .. }`.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Playlist(
    /// Playlist slug or URL
    #[bpaf(positional("PLAYLIST"))]
    String,
);

/// mdma: `ImportPlaylist { name, dry_run, no_wait }`.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct ImportOpts {
    /// MDMA playlist name (default: sanitised source title)
    #[bpaf(long, argument("NAME"))]
    name: Option<String>,
    /// Show the report without syncing or writing the playlist
    #[bpaf(long("dry-run"), switch)]
    dry_run: bool,
    /// Print the job id and exit once the import has started
    #[bpaf(long("no-wait"), switch)]
    no_wait: bool,
}

// --- the tree -----------------------------------------------------------------

/// Music source management
#[group(args = SourceName)]
pub mod source {
    use super::*;

    /// List available music sources
    #[command]
    pub fn list(ctx: &Ctx) -> Outcome {
        ctx.ran.borrow_mut().push((None, "list", Vec::new()));
        Ok(())
    }

    /// Sync the source (download new items)
    #[command(curried)]
    pub fn sync(source: SourceName, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "sync", Vec::new())
    }

    /// Show source status
    #[command(curried)]
    pub fn status(source: SourceName, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "status", Vec::new())
    }

    /// List downloads for the source
    #[command(curried)]
    pub fn downloads(source: SourceName, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "downloads", Vec::new())
    }

    /// Cancel a download
    #[command(curried)]
    pub fn cancel(source: SourceName, id: DownloadId, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "cancel", vec![id.0])
    }

    /// Pause all downloads for the source
    #[command(curried)]
    pub fn pause(source: SourceName, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "pause", Vec::new())
    }

    /// Resume downloads for the source
    #[command(curried)]
    pub fn resume(source: SourceName, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "resume", Vec::new())
    }

    /// Force re-sync of a specific item (bypasses dedup)
    #[command(curried)]
    pub fn resync(source: SourceName, item: Ident, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "resync", vec![item.0])
    }

    /// Check if a specific item has changed upstream
    #[command(curried)]
    pub fn check_item(source: SourceName, item: Ident, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "check-item", vec![item.0])
    }

    /// Check the whole source collection for stale items
    #[command(curried)]
    pub fn check_updates(source: SourceName, apply: Apply, ctx: &Ctx) -> Outcome {
        ctx.record(
            &source,
            "check-updates",
            vec![format!("apply={}", apply.apply)],
        )
    }

    /// List the source's playlists (for sources that support playlists)
    #[command(curried)]
    pub fn playlists(source: SourceName, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "playlists", Vec::new())
    }

    /// Import a source playlist as an MDMA playlist (definitive matches only)
    // The positional `Playlist` is declared *after* `ImportOpts`: hicli
    // composes owned parameters in declaration order, and bpaf requires every
    // positional to come after every option. Declared mdma's way round
    // (`playlist` first, as in its clap struct), rendering `--help` panics, in
    // any build, with bpaf's "all positional and command items must be placed
    // in the right most position" — and it is *any* help in the group
    // (`source --help` too), not just this command's. A debug build catches
    // it early: the tree panics when it is folded. On the command line the
    // order is free: `import-playlist my-list --name x` parses.
    #[command(curried)]
    pub fn import_playlist(
        source: SourceName,
        opts: ImportOpts,
        playlist: Playlist,
        ctx: &Ctx,
    ) -> Outcome {
        ctx.record(
            &source,
            "import-playlist",
            vec![
                playlist.0,
                format!("name={:?}", opts.name),
                format!("dry_run={}", opts.dry_run),
                format!("no_wait={}", opts.no_wait),
            ],
        )
    }

    /// Show the current or last playlist import job, following it while running
    #[command(curried)]
    pub fn import_status(source: SourceName, ctx: &Ctx) -> Outcome {
        ctx.record(&source, "import-status", Vec::new())
    }

    /// Downloads of a source, one at a time
    #[group(curried = SourceName)]
    pub mod download {
        use super::*;

        /// Retry a failed download
        #[command(curried)]
        pub fn retry(source: SourceName, id: DownloadId, ctx: &Ctx) -> Outcome {
            ctx.record(&source, "download retry", vec![id.0])
        }
    }
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [source] }
}

// --- seams ----------------------------------------------------------------------

/// Run `args` through the whole run path; the exit code and what ran.
fn run(args: &[&str]) -> (u8, Vec<Ran>) {
    let ran = Rc::new(RefCell::new(Vec::new()));
    let for_ctx = Rc::clone(&ran);
    let code = app().run_with_args(args.iter().copied(), move |()| -> Result<Ctx, Failure> {
        Ok(Ctx { ran: for_ctx })
    });
    let ran = ran.borrow().clone();
    (code, ran)
}

/// The selected command's path.
fn path(args: &[&str]) -> Vec<&'static str> {
    match app().parse(args.iter().copied()) {
        Ok((_, inv)) => inv.path().to_vec(),
        Err(e) => panic!("{args:?} did not parse: {e:?}"),
    }
}

/// The usage error for `args`, asserting it is one (stderr, exit code 2).
fn usage_error(args: &[&str]) -> String {
    match app().parse(args.iter().copied()) {
        Ok((_, inv)) => panic!("{args:?} parsed, as {:?}", inv.path()),
        Err(e @ ParseFailure::Stderr(..)) => {
            assert_eq!(exit_code_for(&e), 2, "{args:?}");
            e.unwrap_stderr()
        }
        Err(e) => panic!("{args:?}: not a usage error: {e:?}"),
    }
}

/// The help page for `args` (which end in `--help`).
fn help(args: &[&str]) -> String {
    match app().parse(args.iter().copied()) {
        Ok((_, inv)) => panic!("{args:?} parsed, as {:?}", inv.path()),
        Err(e @ ParseFailure::Stdout(..)) => e.unwrap_stdout(),
        Err(e) => panic!("{args:?}: not help: {e:?}"),
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_string()).collect()
}

// --- parsing: mdma's `parse_source_args` tests ----------------------------------

#[test]
fn every_per_source_command_parses_with_the_name_first() {
    // (argv after `source`, the command name, the args it received)
    let cases: Vec<(Vec<&str>, &str, Vec<String>)> = vec![
        (vec!["bandcamp", "sync"], "sync", vec![]),
        (vec!["bandcamp", "status"], "status", vec![]),
        (vec!["bandcamp", "downloads"], "downloads", vec![]),
        (vec!["bandcamp", "cancel", "d1"], "cancel", s(&["d1"])),
        (vec!["bandcamp", "pause"], "pause", vec![]),
        (vec!["bandcamp", "resume"], "resume", vec![]),
        (vec!["bandcamp", "resync", "p1"], "resync", s(&["p1"])),
        (
            vec!["bandcamp", "check-item", "p1"],
            "check-item",
            s(&["p1"]),
        ),
        (
            vec!["bandcamp", "check-updates", "--apply"],
            "check-updates",
            s(&["apply=true"]),
        ),
        (vec!["bandcamp", "playlists"], "playlists", vec![]),
        (
            vec![
                "bandcamp",
                "import-playlist",
                "my-list",
                "--name",
                "x",
                "--dry-run",
                "--no-wait",
            ],
            "import-playlist",
            s(&[
                "my-list",
                "name=Some(\"x\")",
                "dry_run=true",
                "no_wait=true",
            ]),
        ),
        (vec!["bandcamp", "import-status"], "import-status", vec![]),
    ];
    for (rest, command, args) in cases {
        let mut argv = vec!["source"];
        argv.extend_from_slice(&rest);
        let (code, ran) = run(&argv);
        assert_eq!(code, 0, "{argv:?}");
        assert_eq!(
            ran,
            vec![(Some("bandcamp".to_string()), command, args)],
            "{argv:?}"
        );
        // The path names the command, never the value.
        assert_eq!(path(&argv), ["source", command], "{argv:?}");
    }
}

#[test]
fn import_playlist_defaults_have_no_flags_set() {
    let (code, ran) = run(&["source", "bandcamp", "import-playlist", "p"]);
    assert_eq!(code, 0);
    assert_eq!(
        ran,
        vec![(
            Some("bandcamp".to_string()),
            "import-playlist",
            s(&["p", "name=None", "dry_run=false", "no_wait=false"]),
        )]
    );
}

#[test]
fn source_list_parses_as_the_fixed_command() {
    assert_eq!(path(&["source", "list"]), ["source", "list"]);
    let (code, ran) = run(&["source", "list"]);
    assert_eq!(code, 0);
    assert_eq!(ran, vec![(None, "list", vec![])]);
}

#[test]
fn unknown_per_source_command_fails() {
    let (code, ran) = run(&["source", "bandcamp", "frobnicate"]);
    assert_eq!(code, 2);
    assert!(ran.is_empty());
    let _ = usage_error(&["source", "bandcamp", "frobnicate"]);
}

#[test]
fn source_name_without_command_fails() {
    // #27: the command is what is missing, not the value.
    let msg = usage_error(&["source", "bandcamp"]);
    assert!(
        msg.starts_with("expected a command after `bandcamp`: `sync`, `status`, "),
        "{msg}"
    );
    assert_eq!(run(&["source", "bandcamp"]).0, 2);
}

#[test]
fn old_name_last_order_is_rejected() {
    // mdma only asserts the rejection. `sync` is read as the value and
    // `bandcamp` is no command after it — which says what the order is.
    let msg = usage_error(&["source", "sync", "bandcamp"]);
    assert!(
        msg.starts_with("expected a command after `sync`: `sync`, "),
        "{msg}"
    );
    assert_eq!(run(&["source", "sync", "bandcamp"]).0, 2);
}

#[test]
fn a_fixed_sibling_is_never_a_source_name() {
    // mdma: "'list' is reserved and cannot be a source name."
    assert_eq!(
        usage_error(&["source", "list", "sync"]),
        "`sync` is not expected in this context"
    );
}

#[test]
fn a_typo_of_the_fixed_sibling_is_suggested() {
    assert_eq!(
        usage_error(&["source", "lst"]),
        "no such command or positional: `lst`, did you mean `list`?"
    );
}

#[test]
fn a_nested_curried_group_receives_the_value() {
    let (code, ran) = run(&["source", "beatport", "download", "retry", "d9"]);
    assert_eq!(code, 0);
    assert_eq!(
        ran,
        vec![(Some("beatport".to_string()), "download retry", s(&["d9"]))]
    );
    assert_eq!(
        path(&["source", "beatport", "download", "retry", "d9"]),
        ["source", "download", "retry"]
    );
}

#[test]
fn a_curried_command_is_still_the_function_you_wrote() {
    let ctx = Ctx::new();
    source::sync(SourceName("bandcamp".into()), &ctx).expect("ok");
    source::cancel(SourceName("discogs".into()), DownloadId("d1".into()), &ctx).expect("ok");
    assert_eq!(
        *ctx.ran.borrow(),
        vec![
            (Some("bandcamp".to_string()), "sync", vec![]),
            (Some("discogs".to_string()), "cancel", s(&["d1"])),
        ]
    );
}

// --- help -----------------------------------------------------------------------

#[test]
fn source_help_lists_list_and_explains_the_name_first_form() {
    let h = help(&["source", "--help"]);
    assert!(h.contains("SOURCE-NAME COMMAND"), "{h}");
    assert!(h.contains("Commands for SOURCE-NAME:"), "{h}");
    assert!(
        h.contains("Available commands:\n    list ") && h.contains("List available music sources"),
        "{h}"
    );
    // `list` is listed once, as a fixed command, not among the curried ones.
    let section = h
        .split("Commands for SOURCE-NAME:")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .expect("the section");
    assert!(
        !section.lines().any(|l| l.trim_start().starts_with("list ")),
        "{section}"
    );
}

/// #20: the value's field doc is a positional item, not a command row.
///
/// `SourceName`'s own doc comment titles its section (bpaf's rule for a doc
/// comment on a derived struct), so the positional section here is headed by
/// it rather than by "Available positional items:".
#[test]
fn source_help_lists_the_value_with_the_positional_items() {
    let h = help(&["source", "--help"]);
    let paragraphs: Vec<&str> = h.split("\n\n").collect();
    let row = |l: &&str| l.trim_start().starts_with("SOURCE-NAME ");
    let commands = paragraphs
        .iter()
        .find(|p| p.starts_with("Commands for SOURCE-NAME:"))
        .unwrap_or_else(|| panic!("no commands section in {h}"));
    assert!(!commands.lines().any(|l| row(&l)), "{h}");
    let positionals = paragraphs
        .iter()
        .find(|p| p.starts_with("A music source's name"))
        .unwrap_or_else(|| panic!("no positional section in {h}"));
    assert!(
        positionals
            .lines()
            .any(|l| row(&l) && l.ends_with("the music source")),
        "{h}"
    );
    assert_eq!(h.lines().filter(row).count(), 1, "{h}");
}

#[test]
fn source_help_lists_every_per_source_command_under_the_value() {
    // mdma's `per_source_help_lists_the_commands` asks `source bandcamp
    // --help`; in hicli that is the group's own page (a documented limit),
    // which lists the same commands — in their own section. mdma's extra
    // `help.contains("mdma source bandcamp")` has no hicli equivalent: the page
    // does not echo the value.
    for args in [&["source", "--help"][..], &["source", "bandcamp", "--help"]] {
        let h = help(args);
        let section = h
            .split("Commands for SOURCE-NAME:")
            .nth(1)
            .and_then(|rest| rest.split("\n\n").next())
            .unwrap_or_else(|| panic!("no section in {h}"));
        for cmd in [
            "sync",
            "status",
            "downloads",
            "cancel",
            "pause",
            "resume",
            "resync",
            "check-item",
            "check-updates",
            "playlists",
            "import-playlist",
            "import-status",
            "download",
        ] {
            assert!(
                section.lines().any(|l| l.trim_start().starts_with(cmd)),
                "missing {cmd} in {h}"
            );
        }
    }
}

#[test]
fn a_curried_commands_usage_names_the_value() {
    let h = help(&["source", "bandcamp", "sync", "--help"]);
    assert!(h.contains("Usage: source SOURCE-NAME sync"), "{h}");
    let h = help(&["source", "bandcamp", "import-playlist", "--help"]);
    assert!(
        h.contains("Usage: source SOURCE-NAME import-playlist"),
        "{h}"
    );
    assert!(h.contains("--dry-run"), "{h}");
    assert!(h.contains("PLAYLIST"), "{h}");
}

// --- completion -----------------------------------------------------------------

#[test]
fn a_typed_prefix_of_the_fixed_sibling_completes_to_it() {
    assert_eq!(app().complete(cli! { source l }), ["list"]);
}

#[test]
fn a_typed_prefix_of_a_value_completes_from_the_types_completer() {
    assert_eq!(app().complete(cli! { source b }), ["bandcamp", "beatport"]);
}

/// The curried commands, in source order — what follows a value.
const PER_SOURCE: [&str; 13] = [
    "sync",
    "status",
    "downloads",
    "cancel",
    "pause",
    "resume",
    "resync",
    "check-item",
    "check-updates",
    "playlists",
    "import-playlist",
    "import-status",
    "download",
];

/// Regression: bpaf's revision-1 payload has no header line, and
/// `App::complete` once dropped its first candidate — `sync` here, `list` at
/// `source ""`.
#[test]
fn after_the_value_every_curried_command_is_offered() {
    assert_eq!(app().complete(cli! { source bandcamp "" }), PER_SOURCE);
    let got = app().complete(cli! { source "" });
    assert!(got.contains(&"list".to_string()), "{got:?}");
}

#[test]
fn the_fixed_sibling_is_not_offered_after_the_value() {
    let got = app().complete(cli! { source bandcamp "" });
    assert!(!got.contains(&"list".to_string()), "{got:?}");
}

/// Known limit (documented in `src/lib.rs`): at the empty `SOURCE-NAME` slot
/// bpaf also offers the curried commands, which are not valid until a value
/// is typed. Pinned so a fix shows up here.
#[test]
fn known_limit_the_empty_value_slot_also_offers_the_curried_commands() {
    let mut want = vec!["list", "bandcamp", "beatport", "discogs"];
    want.extend(PER_SOURCE);
    assert_eq!(app().complete(cli! { source "" }), want);
}

/// A place, completed from a list in which `l` matches exactly one value —
/// the fixture for the known limit below.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_places)]
pub struct Place(
    /// the place
    #[bpaf(positional("PLACE"))]
    String,
);

/// The places on offer, filtered by what has been typed.
fn complete_places(typed: &Place) -> Vec<Candidate> {
    ["local", "remote"]
        .iter()
        .filter(|c| c.starts_with(typed.0.as_str()))
        .map(|c| ((*c).to_string(), None))
        .collect()
}

/// Places, with a fixed `list` beside the curried `sync`
#[group(args = Place)]
pub mod places {
    use super::*;

    /// List the places
    #[command]
    pub fn list(ctx: &Ctx) -> Outcome {
        ctx.ran.borrow_mut().push((None, "list", Vec::new()));
        Ok(())
    }

    /// Sync a place
    #[command(curried)]
    pub fn sync(place: Place, ctx: &Ctx) -> Outcome {
        ctx.ran
            .borrow_mut()
            .push((Some(place.0), "sync", Vec::new()));
        Ok(())
    }
}

/// Known limit: when the value's completer has exactly one match for a typed
/// word (`l` → `local`), a fixed sibling that also starts with it (`list`) is
/// not offered — bpaf drops the metavariable item that carries it. Pinned so
/// a fix shows up here.
#[test]
fn known_limit_a_lone_value_match_hides_the_fixed_sibling() {
    let app: App<(), Ctx> = app! { globals: pure(()), commands: [places] };
    assert_eq!(app.complete(cli! { places l }), ["local"]);
}

/// A typo of a curried command after a value gets a `did you mean` (#27).
#[test]
fn a_typo_after_the_value_is_suggested() {
    assert_eq!(
        usage_error(&["source", "bandcamp", "snyc"]),
        "no such command after `bandcamp`: `snyc`, did you mean `sync`?"
    );
}
