//! `#[derive(Choice)]` end to end (issue #8, step 1): the derive's three impls,
//! and a choice completed and parsed inside a `#[command]` through a
//! hand-written `Arg` over the string-level chain.
//!
//! Everything here is in-process; nothing spawns a subprocess.

use hicli::bpaf::{long, positional, pure, Parser};
use hicli::{app, cli, command, complete_choice, App, Arg, Choice, Shell, UnknownChoice};

/// The shared context: nothing here needs one.
pub struct Ctx;

/// What to sort by. Private, so its last two variants can go undocumented:
/// that is what a `None` description looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
enum SortKey {
    /// Beats per minute
    Bpm,
    /// How often it was played
    PlayCount,
    #[choice(name = "by-artist")]
    Artist,
    Title = 7,
}

/// The key a listing is sorted by, as a positional.
struct Sort(SortKey);

impl Arg for Sort {
    fn parser() -> impl Parser<Self> {
        positional::<String>("KEY")
            .complete(|w: &String| complete_choice::<SortKey>(w))
            .parse(|w: String| w.parse::<SortKey>())
            .map(Sort)
    }
}

/// The shell to target, as an option.
struct For(Shell);

impl Arg for For {
    fn parser() -> impl Parser<Self> {
        long("for")
            .argument::<String>("SHELL")
            .complete(|w: &String| complete_choice::<Shell>(w))
            .parse(|w: String| w.parse::<Shell>())
            .map(For)
    }
}

/// sort the listing
#[command]
fn sort(s: Sort, ctx: &Ctx) {
    let _ = (s.0, ctx);
}

/// install completions
#[command]
fn install(f: For, ctx: &Ctx) {
    let _ = (f.0, ctx);
}

fn app() -> App<(), Ctx> {
    app! { globals: pure(()), commands: [sort, install] }
}

#[test]
fn the_derive_names_every_variant() {
    let names: Vec<&str> = SortKey::VARIANTS.iter().map(Choice::name).collect();
    assert_eq!(names, ["bpm", "play-count", "by-artist", "title"]);
    for k in SortKey::VARIANTS {
        assert_eq!(k.to_string(), k.name());
        assert_eq!(k.name().parse::<SortKey>(), Ok(*k));
    }
    assert_eq!(SortKey::Bpm.description(), Some("Beats per minute"));
    assert_eq!(SortKey::Artist.description(), None);
    // The explicit discriminant is untouched.
    assert_eq!(SortKey::Title as u8, 7);
}

#[test]
fn matching_is_exact_and_the_error_suggests() {
    let e = "Bpm".parse::<SortKey>().expect_err("case-sensitive");
    assert_eq!(
        e.to_string(),
        "expected `bpm`, `play-count`, `by-artist` or `title`; did you mean `bpm`?"
    );
    assert_eq!(e, UnknownChoice::new::<SortKey>("Bpm"));
    assert_eq!(
        "genre".parse::<SortKey>().expect_err("unknown").to_string(),
        "expected `bpm`, `play-count`, `by-artist` or `title`"
    );
}

#[test]
fn a_choice_completes_inside_a_command() {
    assert_eq!(app().complete(cli! { sort b }), ["bpm", "by-artist"]);
    assert_eq!(app().complete(cli! { sort bp }), ["bpm"]);
    assert_eq!(app().complete(cli! { sort p }), ["play-count"]);
    assert_eq!(
        app().complete(cli! { sort "" }),
        ["bpm", "play-count", "by-artist", "title"]
    );
}

#[test]
fn a_choice_option_completes_inside_a_command() {
    assert_eq!(app().complete(cli! { install --for z }), ["zsh"]);
    assert_eq!(
        app().complete(cli! { install --for "" }),
        ["bash", "zsh", "fish"]
    );
}

#[test]
fn a_choice_parses_and_reports_inside_a_command() {
    let (_, inv) = app().parse(cli! { sort play-count }).expect("parses");
    assert_eq!(inv.path(), ["sort"]);
    let e = app()
        .parse(cli! { sort bmp })
        .err()
        .expect("not a sort key")
        .unwrap_stderr();
    assert_eq!(
        e,
        "couldn't parse `bmp`: expected `bpm`, `play-count`, `by-artist` or `title`; \
         did you mean `bpm`?"
    );
    let e = app()
        .parse(cli! { install --for powershell })
        .err()
        .expect("not a shell")
        .unwrap_stderr();
    assert_eq!(
        e,
        "couldn't parse `powershell`: expected `bash`, `zsh` or `fish`"
    );
}

#[test]
fn the_run_path_leaves_a_choice_error_alone() {
    // `did you mean` is already in the message, so the top-level gap-closer
    // adds nothing; the exit code is a usage error's.
    assert_eq!(app().run_with_args(cli! { sort bmp }, |_| Ok(Ctx)), 2);
    assert_eq!(app().run_with_args(cli! { sort bpm }, |_| Ok(Ctx)), 0);
}
