//! The terminal-shaped test macros: [`cli!`] and [`verb_test!`].
//!
//! These let a test show, verbatim, the command line the user types.

/// Build an argv `Vec<String>` from **raw command-line tokens** — no wrapping
/// string literal — so the call site reads like typing in a terminal:
///
/// ```
/// # use verbandi::cli;
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
/// the parse `Result<(G, BoxedVerb<Ctx>), bpaf::ParseFailure>`. A leading
/// `app: <expr>;` header supplies the consumer's `App` builder, evaluated
/// fresh for every case.
///
/// ```
/// # use verbandi::{verb_test, App, Group, Verb, BoxedVerb, Exit, exit_code_for};
/// # use verbandi::bpaf::{Parser, positional, pure};
/// # struct Ctx;
/// # struct Create { name: Vec<String> }
/// # impl Verb<Ctx> for Create {
/// #     fn name(&self) -> &'static str { "create" }
/// #     fn run(self: Box<Self>, _c: &Ctx) -> Result<(), Exit> { Ok(()) }
/// # }
/// # fn create() -> impl Parser<BoxedVerb<Ctx>> {
/// #     let _dir = positional::<String>("DIR");
/// #     let name = positional::<String>("NAME").some("a name is required");
/// #     verbandi::bpaf::construct!(_dir, name)
/// #         .map(|(_dir, name)| Box::new(Create { name }) as BoxedVerb<Ctx>)
/// # }
/// # fn app() -> App<(), Ctx> {
/// #     let group = Group::new().leaf("create", "create an event", create());
/// #     App::new(pure(())).group("event", "manage events", group)
/// # }
/// verb_test! {
///     app: app();
///     create_ok: { event create x "My Event" } => |r| {
///         let (_g, verb) = r.expect("parses");
///         assert_eq!(verb.name(), "create");
///     },
///     missing_name_usage: { event create x } => |r| {
///         let e = r.unwrap_err();
///         assert_eq!(exit_code_for(&e), 2);
///         assert!(e.unwrap_stderr().contains("NAME"));
///     },
/// }
/// ```
#[macro_export]
macro_rules! verb_test {
    (app: $app:expr; $($name:ident: { $($line:tt)* } => $body:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                $crate::testing::run_case($app, $crate::cli!($($line)*), $body);
            }
        )+
    };
}
