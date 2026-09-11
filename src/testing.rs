//! Test-support helpers used by the [`cli!`](crate::cli) and
//! [`verb_test!`](crate::verb_test) macros.

/// Drive one [`verb_test!`](crate::verb_test) case.
///
/// Not part of the public API surface for direct use — it exists so the macro
/// can give the case's closure a fully-known argument type (the parse result),
/// which plain closure inference cannot recover on its own.
#[doc(hidden)]
pub fn run_case<G, Ctx, F>(app: crate::App<G, Ctx>, args: Vec<String>, check: F)
where
    G: 'static,
    Ctx: 'static,
    F: FnOnce(Result<(G, crate::BoxedVerb<Ctx>), bpaf::ParseFailure>),
{
    check(app.parse(args));
}

/// Convert a single literal token to its argv string.
///
/// Called by [`cli!`](crate::cli) for literal tokens: a string literal yields
/// its value verbatim (`"My Event"` -> `My Event`), and numeric/char/bool
/// literals yield their `Display` form (`5` -> `5`). This is what lets a
/// spaced argument be written as a double-quoted string literal token.
pub fn to_arg(x: impl std::fmt::Display) -> String {
    x.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_arg_string_verbatim_number_display() {
        assert_eq!(to_arg("My Event"), "My Event");
        assert_eq!(to_arg(5), "5");
    }
}
