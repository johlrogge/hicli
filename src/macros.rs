//! The declarative macros: [`app!`] for assembling the command tree, [`cli!`]
//! and [`cli_test!`] for driving it.
//!
//! The command tree itself is declared with the two attributes
//! [`#[command]`](macro@crate::command) and [`#[group]`](macro@crate::group),
//! which are proc macros and live in `hicli-macros`. What is left here is
//! declarative because it does not have to rewrite an ident.
//!
//! [`app!`] is sugar over [`App`](crate::App) and
//! [`commands!`](crate::commands) — everything it expands to is public API, so
//! a tree that outgrows it can be written by hand and nothing is lost. The last
//! two let a test show, verbatim, the command line the user types.
//!
//! [`app!`] forwards its list as **token trees**, never as `$c:expr`. A matched
//! `:expr` reaches [`commands!`](crate::commands) wrapped in an invisible group
//! — a single opaque token tree that says "this is an expression", which is
//! precisely the thing `commands!` has to tell apart from a bare path naming a
//! handle. `commands!` peels the groups it can see, so a forwarder built the
//! other way still works today; forwarding token trees is what keeps the list
//! *literally* the list the user wrote. A consumer writing a forwarder of their
//! own should do the same.

/// Build an [`App`](crate::App) from a globals parser and a list of top-level
/// commands.
///
/// Expands to `App::new(<globals>).commands(commands![…])`, so `run` stays a
/// method call and `main` reads `app! { … }.run(mk_ctx)`. The list follows
/// [`commands!`](crate::commands)' rule: a bare path names a handle.
///
/// ```
/// # use hicli::bpaf::pure;
/// # use hicli::{app, cli, App};
/// # struct Ctx;
/// /// show status
/// #[hicli::command]
/// fn status(ctx: &Ctx) {
///     let _ = ctx;
/// }
///
/// fn main() {
///     let a: App<(), Ctx> = app! {
///         globals: pure(()),
///         commands: [status],
///     };
///     let (_globals, invocation) = a.parse(cli! { status }).expect("parses");
///     assert_eq!(invocation.path(), ["status"]);
/// }
/// ```
///
/// The two keys are required and ordered: `globals` first, then `commands`.
/// Anything more elaborate — a globals struct assembled in place, commands
/// added conditionally — is what the [`App`](crate::App) builder is for.
///
/// The command list is matched as **token trees** and handed to `commands!`
/// verbatim: matching it as `$c:expr` would wrap each element in an invisible
/// group, which is how a list says "these are expressions" — and a bare path
/// has to stay a *name*. A consumer writing their own forwarder around
/// `commands!` has to forward token trees for the same reason.
#[macro_export]
macro_rules! app {
    (globals: $globals:expr, commands: [ $($c:tt)* ] $(,)?) => {
        $crate::App::new($globals).commands($crate::commands![$($c)*])
    };
}

/// Build an argv `Vec<String>` from **raw command-line tokens** — no wrapping
/// string literal — so the call site reads like typing in a terminal:
///
/// ```
/// # use hicli::cli;
/// let args = cli! { event create x "My Event" };
/// assert_eq!(args, vec!["event", "create", "x", "My Event"]);
/// ```
///
/// # How tokens map to argv words
/// - A **single-token** argument needs no quotes: `event`, `create`, `x`, `42`.
/// - A **spaced or special** argument is written as a **double-quoted string
///   literal**, whose value is taken verbatim: `"My Event"` -> `My Event`.
/// - **Flags** glue their dashes to the following identifier: `--home` ->
///   `--home`, `-h` -> `-h`.
///
/// # What does NOT work (Rust lexer limits — important)
/// `cli!` walks *source tokens*, so it is bound by Rust's lexer:
/// - `'My Event'` does **not** compile: Rust lexes a leading `'` as the start
///   of a lifetime (`'My`), not a quoted word. **Use double quotes**:
///   `"My Event"`.
/// - A value containing punctuation that would lex as several tokens
///   (e.g. `email,phone`, `a/b/c`) must be double-quoted: `"email,phone"`,
///   `"a/b/c"`. A bare identifier or number is fine unquoted.
///
/// The result is a `Vec<String>`, which plugs straight into
/// [`App::parse`](crate::App::parse).
#[macro_export]
macro_rules! cli {
    ($($tokens:tt)*) => {{
        let mut __args: ::std::vec::Vec<::std::string::String> = ::std::vec::Vec::new();
        $crate::__cli_munch!(__args ; $($tokens)*);
        __args
    }};
}

/// Internal token-muncher for [`cli!`]. Not part of the public API.
#[macro_export]
#[doc(hidden)]
macro_rules! __cli_munch {
    // done
    ($v:ident ; ) => {};
    // long flag: `--ident`
    ($v:ident ; - - $flag:ident $($rest:tt)*) => {
        $v.push(::std::format!("--{}", ::std::stringify!($flag)));
        $crate::__cli_munch!($v ; $($rest)*);
    };
    // short flag: `-ident`
    ($v:ident ; - $flag:ident $($rest:tt)*) => {
        $v.push(::std::format!("-{}", ::std::stringify!($flag)));
        $crate::__cli_munch!($v ; $($rest)*);
    };
    // negative number (or other leading-dash literal): `-5`
    ($v:ident ; - $lit:literal $($rest:tt)*) => {
        $v.push(::std::format!("-{}", $crate::testing::to_arg($lit)));
        $crate::__cli_munch!($v ; $($rest)*);
    };
    // literal token (string/number/char/bool) -> value verbatim
    ($v:ident ; $lit:literal $($rest:tt)*) => {
        $v.push($crate::testing::to_arg($lit));
        $crate::__cli_munch!($v ; $($rest)*);
    };
    // any other bare token (ident, path segment, punctuation) -> stringified
    ($v:ident ; $t:tt $($rest:tt)*) => {
        $v.push(::std::string::String::from(::std::stringify!($t)));
        $crate::__cli_munch!($v ; $($rest)*);
    };
}

/// Generate one `#[test]` per typed command line.
///
/// The left side of each case is the **raw tokens** the user types (grouped in
/// braces, same rules as [`cli!`]); the right side is a closure that receives
/// the parse `Result<(G, Invocation<Ctx>), bpaf::ParseFailure>`. A leading
/// `app: <expr>;` header supplies the consumer's `App` builder, evaluated
/// fresh for every case.
///
/// ```
/// # use hicli::{cli_test, App, Command, exit_code_for};
/// # use hicli::bpaf::{Parser, positional, pure};
/// # struct Ctx;
/// # fn create() -> Command<Ctx> {
/// #     let dir = positional::<String>("DIR");
/// #     let name = positional::<String>("NAME").some("a name is required");
/// #     let args = hicli::bpaf::construct!(dir, name);
/// #     hicli::command("create", "create an event", args,
/// #                    |(_dir, _name): (String, Vec<String>), _c: &Ctx| {})
/// # }
/// # fn app() -> App<(), Ctx> {
/// #     App::new(pure(())).command(hicli::group("event", "manage events", vec![create()]))
/// # }
/// cli_test! {
///     app: app();
///     create_ok: { event create x "My Event" } => |r| {
///         let (_g, invocation) = r.expect("parses");
///         assert_eq!(invocation.path(), ["event", "create"]);
///     },
///     missing_name_usage: { event create x } => |r| {
///         let e = r.unwrap_err();
///         assert_eq!(exit_code_for(&e), 2);
///         assert!(e.unwrap_stderr().contains("NAME"));
///     },
/// }
/// ```
#[macro_export]
macro_rules! cli_test {
    (app: $app:expr; $($name:ident: { $($line:tt)* } => $body:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                $crate::testing::run_case($app, $crate::cli!($($line)*), $body);
            }
        )+
    };
}
