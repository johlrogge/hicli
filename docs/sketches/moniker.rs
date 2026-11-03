//! SKETCH — what `projects/moniker/src/` would look like on hicli, after
//! ADR-005 (handles beside the function, groups as modules) and ADR-007 (the
//! `hicli`/`#[command]` vocabulary).
//!
//! Not compiled. This is several files, shown in one, with `// ===== path =====`
//! banners. Lines marked `// ?` are choices worth reacting to.
//!
//! Layout on disk:
//!
//!     src/main.rs        globals, context, the top-level tree, the tests
//!     src/args.rs        the shared `#[derive(Bpaf)]` argument structs
//!     src/event.rs       the `event` group + the nested `audience` group
//!     src/id.rs          the `id` group
//!     src/wallet.rs      the `wallet` group
//!     src/top.rs         top-level commands (not a group)
//!
//! Shown: a representative slice of moniker's ~40 commands, one of each kind:
//!   - shared argument structs reused across commands  (`EventRef`, `EventDir`)
//!   - a file module as a group                        (`event create`, `event roster`)
//!   - an inline nested group                          (`event audience define --seal-roster`)
//!   - a top-level command with several arg structs    (`apply … --share --unshare --voucher`)
//!   - a command with an argument invariant            (`id recover WORD…` must be 24 words)
//!   - a command that bypasses the session             (`id create`)

// ===========================================================================
// ===== src/main.rs =========================================================
// ===========================================================================

use std::path::PathBuf;
use std::process::ExitCode;

use cli::Session; // bases/cli — unchanged, parser-free
use hicli::bpaf::Bpaf; // argument *structs* derive; commands and groups don't
use hicli::{app, group, App, Failure};

mod args;
mod top;

/// Global options, accepted before or after the subcommand.
///
/// Globals are not a command, so they stay a plain `#[derive(Bpaf)]` struct.
#[derive(Debug, Clone, Bpaf)]
pub struct Globals {
    /// Home directory; falls back to $MONIKER_HOME.
    #[bpaf(long, argument("DIR"))]
    home: Option<String>,
    /// Remote append server; `https://` is assumed if no scheme is given.
    #[bpaf(long, argument("HOST"))]
    remote: Option<String>,
}

/// The shared context every command receives.
///
/// The passphrase is read from the environment in `mk_ctx`, never parsed — it
/// must not appear in `--help` or be settable on argv.
pub struct Ctx {
    pub home: PathBuf,
    pub passphrase: String,
    remote: Option<String>,
}

impl Ctx {
    /// Open (or create) the session. Most commands start here; `id create` and
    /// `id recover` deliberately do not.
    pub fn session(&self) -> Result<Session, Failure> {
        let (session, fresh_phrase) =
            Session::open_or_create(&self.home, &self.passphrase).map_err(Failure::runtime)?;
        if let Some(phrase) = fresh_phrase {
            announce_phrase(&phrase);
        }
        Ok(session)
    }

    pub fn remote(&self) -> Option<&str> {
        self.remote.as_deref()
    }
}

/// Build the context from globals + environment. Runs after parsing, before the
/// command. Every failure here is a usage error (exit 2), as today.
fn mk_ctx(g: Globals) -> Result<Ctx, Failure> {
    let home = g
        .home
        .or_else(|| non_empty_env("MONIKER_HOME"))
        .ok_or_else(|| Failure::usage("--home or MONIKER_HOME is required"))?;
    let passphrase = non_empty_env("MONIKER_PASSPHRASE")
        .ok_or_else(|| Failure::usage("MONIKER_PASSPHRASE must be set"))?;
    let remote = match g.remote {
        Some(r) if r.is_empty() => return Err(Failure::usage("--remote needs a host")),
        Some(r) => Some(resolve_remote(&r)),
        None => None,
    };
    Ok(Ctx { home: PathBuf::from(home), passphrase, remote })
}

// ---------------------------------------------------------------------------
// The groups: one file each, declared here.
//
// `#[group]` on a file-module declaration is the same group as on an inline
// module. It emits `pub mod event;` plus a hidden sibling handle, and discovers
// the group's children by reading `src/event.rs`: every `#[command]` function and
// every `#[group]` module in it, in source order. There is no registration line
// per command — adding a command is adding a function.
// ---------------------------------------------------------------------------

/// events and instances
///
/// An event is a directory; an instance is a branch of it.
#[group]
pub mod event;

/// identity
#[group]
pub mod id;

/// cards and deliverables
#[group]
pub mod wallet;

/// The whole command tree. Named, so the tests below get the same one.
fn app() -> App<Globals, Ctx> {
    // A bare name is a handle: `event` -> `__hicli_event()`, `top::apply` ->
    // `top::__hicli_apply()`. Anything that is not a bare path (a call, a
    // parenthesised local) is an expression producing a `Command`.
    app! {
        globals: globals(),
        commands: [id, event, wallet, top::apply, top::badge],
    }
}

fn main() -> ExitCode {
    // With no tests you would inline `app! { … }.run(mk_ctx)` here.
    app().run(mk_ctx) // returns ExitCode; hicli never calls process::exit
}

// ===========================================================================
// ===== src/args.rs ================================= CHANGED (ADR-006) =====
//
// An argument is a **type**, and the type is the vocabulary. Each one derives
// `Bpaf` (the spelling) and `Arg` (the fact that it *is* an argument, plus its
// completion). Every command that takes an `Id` inherits `Id`'s metavar, its help
// and its completion — write it once, complete it everywhere.
//
// `args.rs` is still an ordinary module — no `#[group]`, no handles.
// ===========================================================================

use hicli::bpaf::{construct, Bpaf, Parser};
use hicli::{Arg, Candidate};

/// `EVENT_DIR` — every event command starts with one.
///
/// `complete = dirs` hands completion to the shell itself, so it is as fast
/// and as correct as `cd`'s.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = dirs)]
pub struct EventDir(
    /// where the event lives
    #[bpaf(positional("EVENT_DIR"))]
    pub String,
);

/// `INSTANCE` — a branch of an event.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Instance(#[bpaf(positional("INSTANCE"))] pub String);

/// `ID` — an identity in this home.
///
/// This is the newtype the whole design is for: `moniker id show <TAB>` lists
/// the ids you actually have, and so does every *other* command that takes an
/// `Id`, for free, forever. The completion is declared here, once.
#[derive(Debug, Clone, Bpaf, Arg)]
#[arg(complete = complete_ids)]
pub struct Id(#[bpaf(positional("ID"))] pub String);

/// The ids under `$MONIKER_HOME`, filtered by what has been typed.
///
/// Completion runs inside the user's shell on every TAB, so this reads the
/// environment (never `--home`: bpaf hands a completion function the partial
/// value and nothing else — ADR-006 §6), never fails, and never panics. An
/// unreadable home is an empty list, not an error. bpaf passes the result
/// through unfiltered, so the prefix filter is ours.
fn complete_ids(partial: &Id) -> Vec<Candidate> {
    let Some(home) = std::env::var_os("MONIKER_HOME") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(std::path::Path::new(&home).join("ids")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.starts_with(&partial.0))
        .map(|name| (name, None))
        .collect()
}

/// `EVENT_DIR INSTANCE` — ~12 of moniker's commands take exactly this pair.
///
/// Composed from two `Arg`s rather than derived, so each half keeps the
/// completion it declared: `EventDir`'s directory completion survives being
/// half of an `EventRef`. A composite whose halves complete nothing can just
/// `#[derive(Bpaf, Arg)]` two positionals instead.              // ? the seam
#[derive(Debug, Clone)]
pub struct EventRef {
    pub event_dir: EventDir,
    pub instance: Instance,
}

impl Arg for EventRef {
    fn parser() -> impl Parser<Self> {
        let event_dir = <EventDir as Arg>::parser();
        let instance = <Instance as Arg>::parser();
        construct!(EventRef { event_dir, instance })
    }
}

/// A multi-word display name: `NAME…`, joined on use.
///
/// Could complete too: under `some(..)` bpaf still calls a completer with the
/// words collected so far, the in-progress one last (`fn(&Vec<String>)`).
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct EventName(#[bpaf(positional("NAME"), some("a name is required"))] pub Vec<String>);

impl EventName {
    pub fn joined(&self) -> String {
        self.0.join(" ")
    }
}

/// `MONIKER` — who is applying / being admitted.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct Moniker(#[bpaf(positional("MONIKER"))] pub String);

/// The card-sharing options, shared by `apply` and `offer`.
///
/// `Arg` is about being *a command's parameter*, not about being positional: an
/// options struct derives it exactly like a newtype. Completion for a *field's*
/// value belongs on that field, with bpaf's own `#[bpaf(complete(f))]` —
/// `#[arg(complete = …)]` describes one whole value and is a compile error here.
#[derive(Debug, Clone, Bpaf, Arg)]
pub struct ShareOpts {
    /// Comma-separated card kinds to share, e.g. email,phone
    #[bpaf(long, argument("KINDS"), complete(complete_kinds), optional)]
    pub share: Option<String>,
    /// Comma-separated card kinds to stop sharing
    #[bpaf(long, argument("KINDS"), optional)]
    pub unshare: Option<String>,
    /// A voucher deliverable to apply with
    #[bpaf(long, argument("DELIVERABLE"), optional)]
    pub voucher: Option<String>,
}

// ===========================================================================
// ===== src/event.rs ========================================================
// ===========================================================================

use std::path::Path;

use hicli::bpaf::Bpaf;
use hicli::{command, group, Failure, Outcome};

use crate::args::*;
use crate::Ctx;

/// create an event
///
/// The event directory is created if it does not exist. On `--remote` the
/// directory argument is the event's genesis-hash id instead of a path.
// ^ first line is the one-line help (parent listing + `--help` heading); the
//   rest becomes the long description. A command with no doc comment is a compile
//   error — help is mandatory (ADR-001), enforced by rustc.
#[command]
pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
    // Every owned parameter is one `#[derive(Bpaf)]` struct, parsed by its
    // derived constructor (`event_dir()`, `event_name()`) and composed with
    // `construct!` in declaration order — which is also the order of the
    // positionals on the command line.
    //
    // `ctx: &Ctx` — the one reference parameter — is the context, and fixes
    // this group's `Ctx` as well as this command's.
    //
    // `#[command]` leaves this function exactly as written: same name, same
    // signature, same visibility. It only *adds* a hidden sibling,
    // `__hicli_create()`, which is what `commands![create]` registers.
    let session = ctx.session()?;
    let report = cli::create_event(&session, Path::new(&dir.0), &name.joined(), ctx.remote())
        .map_err(Failure::runtime)?;
    print_create(&report); // the existing print_* helper, moved next to its command
    Ok(())
}

/// show an instance's roster
#[command]
pub fn roster(ev: EventRef, ctx: &Ctx) -> Outcome {
    let session = ctx.session()?;
    let report = cli::roster(&session, Path::new(&ev.event_dir), &ev.instance, ctx.remote())
        .map_err(Failure::runtime)?;
    print_roster(&report);
    Ok(())
}

// A nested group is a nested module, marked. Because it has a body, the plain
// attribute works on today's toolchain — and rustfmt formats the whole thing.
// Its handle, `__hicli_audience()`, is emitted here beside `mod audience`,
// which is exactly where `event`'s handle looks for it.
/// manage audiences
///
/// An audience is a set of people invited together.
#[group]
pub mod audience {
    use super::*;

    /// options for `audience define`
    #[derive(Debug, Clone, Bpaf)]
    pub struct DefineOpts {
        /// freeze the roster once the audience is defined
        #[bpaf(long("seal-roster"), switch)]
        pub seal_roster: bool,
    }

    // `opts` is declared *first* because bpaf requires every positional to sit
    // at the right-hand end of the structure it is composed into — a rule about
    // the parameter order only: `--seal-roster` is still accepted anywhere.
    /// define an audience
    #[command]
    pub fn define(opts: DefineOpts, dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome {
        let session = ctx.session()?;
        let report = cli::define_audience(
            &session,
            Path::new(&dir.0),
            &name.joined(),
            opts.seal_roster,
            ctx.remote(),
        )
        .map_err(Failure::runtime)?;
        print_define_audience(&report);
        Ok(())
    }

    // pub fn add(…) / remove(…) / show(…) — same shape, elided.
}

// instance, applications, admit, add_admin (-> `add-admin`), remove, revoke,
// mute, unmute, transfer_ownership, report, reports, voucher, invited — each
// one more `#[command] pub fn` in this file, and nothing else anywhere.

#[cfg(test)]
mod tests {
    // `mod tests` is not marked `#[group]`, so nothing in it is discovered —
    // a `#[command]` helper here would never become a command.
    use super::*;

    // The command is a function. Not `create::run`, not a handle: the function,
    // called the way it is declared.
    #[test]
    fn create_makes_the_directory() {
        let ctx = test_ctx();
        create(
            EventDir("./ev".into()),
            EventName(vec!["Summer".into(), "Party".into()]),
            &ctx,
        )
        .expect("creates an event");
        assert!(ctx.home.join("ev").is_dir());
    }
}

// ===========================================================================
// ===== src/id.rs ===========================================================
// ===========================================================================

use hicli::bpaf::Bpaf;
use hicli::{command, Failure, Outcome};

use crate::Ctx;

/// create an identity
#[command]
pub fn create(ctx: &Ctx) -> Outcome {
    // No owned parameters: this command takes no arguments at all. Bypasses
    // `ctx.session()` on purpose.
    //
    // This `create` and `event::create` coexist without a thought: two files,
    // two modules, two functions. `invocation.path()` says which one ran.
    let report = cli::create_identity(&ctx.home, &ctx.passphrase).map_err(Failure::runtime)?;
    print_badge(&report.badge);
    if let Some(phrase) = report.fresh_phrase {
        announce_phrase(&phrase);
    }
    Ok(())
}

/// The 24-word rule as a bpaf `guard`, not an `if` in the body: it fails on the
/// *parse* path, so it renders with usage, exits 2, and shows up in a
/// `cli_test!` case — and the command's body keeps only domain logic.
#[derive(Debug, Clone, Bpaf)]
pub struct RecoveryPhrase(
    /// the 24 recovery words
    #[bpaf(
        positional("WORD"),
        many,
        guard(twenty_four_words, "a recovery phrase is exactly 24 words")
    )]
    pub Vec<String>,
);

/// bpaf's `guard` wants `fn(&T) -> bool`, hence the `&Vec` clippy allow.
#[allow(clippy::ptr_arg)]
fn twenty_four_words(words: &Vec<String>) -> bool {
    words.len() == 24
}

/// recover an identity from its 24 recovery words
#[command]
pub fn recover(phrase: RecoveryPhrase, ctx: &Ctx) -> Outcome {
    let session =
        Session::recover(&ctx.home, &ctx.passphrase, &phrase.0).map_err(Failure::runtime)?;
    println!("recovered {}", session.identity_hex());
    Ok(())
}

/// show an identity
///
/// `id: Id` is the entire completion story for this command. It declares nothing,
/// configures nothing and knows nothing about `$MONIKER_HOME` — it takes an
/// `Id`, and an `Id` completes.
#[command]
pub fn show(id: Id, ctx: &Ctx) -> Outcome {
    print_badge(&ctx.session()?.badge_of(&id.0)?);
    Ok(())
}

// ? The order of `id --help` is the order of this file: create, recover, show.
//   Source order is the only ordering there is — move a function to move a
//   command. The old `id: [create, show, recover]` list could say otherwise.

// ===========================================================================
// ===== src/wallet.rs =======================================================
// ===========================================================================

use hicli::{command, Outcome};

use crate::Ctx;

/// list the cards in your wallet
#[command]
pub fn cards(ctx: &Ctx) -> Outcome {
    print_cards(&ctx.session()?.cards());
    Ok(())
}

// ===========================================================================
// ===== src/top.rs ==========================================================
//
// Top-level commands. NOT a group: `top` is never declared with `#[group]`,
// so it contributes no `top` subcommand — its commands are named
// individually in `main.rs`'s `app!` list as `top::apply`, `top::badge`.
// ===========================================================================

use std::path::Path;

use hicli::{command, Failure, Outcome};

use crate::args::*;
use crate::Ctx;

/// apply to an instance
///
/// Shares the card kinds named by `--share` with the instance's admins.
#[command]
pub fn apply(ev: EventRef, who: Moniker, opts: ShareOpts, ctx: &Ctx) -> Outcome {
    // Three argument structs, all shared with other commands. The signature
    // stays three names wide however many flags `ShareOpts` grows.
    let session = ctx.session()?;
    let report = cli::apply(
        &session,
        Path::new(&ev.event_dir),
        &ev.instance,
        &who.0,
        &split_kinds(opts.share.as_deref()),
        &split_kinds(opts.unshare.as_deref()),
        opts.voucher.as_deref(),
        ctx.remote(),
    )
    .map_err(Failure::runtime)?;
    print_apply(&report);
    Ok(())
}

/// show your badge
#[command]
pub fn badge(ctx: &Ctx) -> Outcome {
    print_badge(&ctx.session()?.badge());
    Ok(())
}

// accept, acceptances, offer, offers, status, block, unblock, blocklist

// ===========================================================================
// ===== src/main.rs — tests =================================================
//
// In-process, the way the user types. Note what is absent: no handle is named
// anywhere. `cli_test!` and `app()` cover the tree; the commands cover
// themselves by being functions.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use hicli::{cli, cli_test, exit_code_for};

    // Parse level: which command, which globals, which exit intent.
    cli_test! {
        app: app();
        create_ok: { event create "./ev" "Summer Party" } => |r| {
            let (g, invocation) = r.expect("parses");
            // `path()` disambiguates `event create` from `id create`.
            assert_eq!(invocation.path(), ["event", "create"]);
            assert!(g.remote.is_none());
        },
        create_needs_name: { event create "./ev" } => |r| {
            let e = r.unwrap_err();
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("a name is required"));
        },
        apply_with_share: { apply "./ev" main jo --share "email,phone" } => |r| {
            let (_g, invocation) = r.expect("parses");
            assert_eq!(invocation.name(), "apply");
        },
        nested_group: { event audience define "./ev" Friends --seal-roster } => |r| {
            let (_g, invocation) = r.expect("parses");
            assert_eq!(invocation.path(), ["event", "audience", "define"]);
        },
        remote_after_command: { badge --remote example.org } => |r| {
            let (g, _invocation) = r.expect("parses");
            assert_eq!(g.remote.as_deref(), Some("example.org"));
        },
        typo_suggests: { event rostre "./ev" main } => |r| {
            let e = r.unwrap_err();
            assert!(e.unwrap_stderr().contains("roster"));
        },
        // the guard fails at parse time, with usage
        recover_needs_24_words: { id recover one two three } => |r| {
            let e = r.unwrap_err();
            assert_eq!(exit_code_for(&e), 2);
            assert!(e.unwrap_stderr().contains("exactly 24 words"));
        },
    }

    // Arg level: an argument struct is an ordinary bpaf parser, tested once and
    // trusted by every command that takes it.
    #[test]
    fn event_name_joins_its_words() {
        let n = args::event_name()
            .to_options()
            .run_inner(&["Summer", "Party"][..])
            .expect("parses");
        assert_eq!(n.joined(), "Summer Party");
    }

    // Body level: the command is still just a function — and now it is *the*
    // function, called by its own name with its own signature.
    #[test]
    fn id_create_does_not_open_a_session() {
        let ctx = Ctx { home: tmp(), passphrase: "x".into(), remote: None };
        id::create(&ctx).expect("creates an identity");
    }

    // Run level: the whole pipeline, exit codes, no subprocess.
    #[test]
    fn missing_passphrase_is_usage() {
        std::env::remove_var("MONIKER_PASSPHRASE");
        // note the quotes: `cli!` walks Rust tokens, so a path is a string literal
        assert_eq!(app().run_with_args(cli! { --home "/tmp/h" badge }, mk_ctx), 2);
    }

    // Completion level: in-process, no shell. The candidates come from the
    // *argument type*, not from anything written at the command.
    #[test]
    fn id_show_completes_ids_from_the_home_directory() {
        let home = fixture_home_with_ids(&["alice", "alfred", "bo"]);
        std::env::set_var("MONIKER_HOME", &home);
        assert_eq!(app().complete(cli! { id show al }), ["alice", "alfred"]);
    }
}

// ===========================================================================
// ===== Appendix: what the macros expand to (you never write this) ==========
// ===========================================================================
//
// `#[command]` on
//
//     /// create an event
//     ///
//     /// The event directory is created if it does not exist.
//     #[command]
//     pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome { BODY }
//
// emits the function unchanged, plus one hidden sibling:
//
//     /// create an event
//     ///
//     /// The event directory is created if it does not exist.
//     pub fn create(dir: EventDir, name: EventName, ctx: &Ctx) -> Outcome { BODY }
//
//     #[doc(hidden)]
//     #[allow(unreachable_pub)]
//     pub fn __hicli_create() -> ::hicli::Command<Ctx> {
//         ::hicli::command(
//             "create",                                   // the ident, `_` -> `-`
//             "create an event",                          // doc comment, first line
//             ::hicli::bpaf::construct!(event_dir(), event_name(),),
//             |(a0, a1), ctx: &Ctx| create(a0, a1, ctx),
//         )
//         .details("The event directory is created if it does not exist.")
//     }
//
// Arity rules: 0 owned params -> `pure(())` and `|(), ctx|`; 1 -> the
// constructor with a trailing comma and `|a0, ctx|`; n -> `construct!` and a
// tuple pattern. No `&Ctx` parameter at all -> `pub fn __hicli_create<Ctx: 'static>()`,
// and the context is ignored. The handle is always `pub` + `#[doc(hidden)]`.
//
// `/// events and instances\n#[group]\npub mod event;` emits
//
//     /// events and instances
//     #[path = "event.rs"]
//     pub mod event;
//
//     #[doc(hidden)]
//     #[allow(unreachable_pub)]
//     pub fn __hicli_event() -> ::hicli::Command<Ctx> {
//         #[allow(unused_mut)]
//         let mut __children: ::std::vec::Vec<::hicli::Command<Ctx>> =
//             ::std::vec::Vec::new();
//         __children.push(event::__hicli_create());
//         __children.push(event::__hicli_roster());
//         __children.push(event::__hicli_audience());
//         ::hicli::group("event", "events and instances", __children)
//     }
//
// `#[group] pub mod audience { … }` emits the module verbatim plus the same
// shape of handle beside it. `commands![a, b::c, expr()]` expands to
//
//     ::std::vec![
//         __hicli_a(),
//         b::__hicli_c(),
//         ::hicli::IntoCommand::into_command(expr()),
//     ]
//
// and `app! { globals: g, commands: [a, b] }` to
// `::hicli::App::new(g).commands(::hicli::commands![a, b])`.
//
// `::hicli::command`, `::hicli::group`, `::hicli::external`,
// `App::new`/`.commands` and the `__hicli_<ident>` handle convention are all
// public API, so every macro is sugar over something you could write by hand —
// including a group that needs a hand-built child:
//
//     /// events and instances
//     #[doc(hidden)]
//     pub fn __hicli_event() -> Command<Ctx> {
//         hicli::group("event", "events and instances",
//             commands![event::create, hicli::external(foo())])
//     }
//
// ===========================================================================
// ===== FUTURE, not v1: "currying" ==========================================
// ===========================================================================
//
// `moniker event ./ev main create "Summer Party"`: the group parses `EventRef`
// once and hands it to whichever child ran. The child's *body* does not change
// — `EventRef` is already one of its parameters — only where the value came
// from. The slot is on the group's declaration:
//
//     /// events and instances
//     #[group(args = EventRef)]
//     pub mod event { … }
//
// Nothing in the consumer-facing shape changes. See ADR-004's "Future work" for
// what the primitives must look like today so this stays additive.
