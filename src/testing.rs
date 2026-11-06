//! Test-support helpers used by the [`cli_test!`](crate::cli_test) macro.

/// Drive one [`cli_test!`](crate::cli_test) case.
///
/// Not part of the public API surface for direct use — it exists so the macro
/// can give the case's closure a fully-known argument type (the parse result),
/// which plain closure inference cannot recover on its own.
#[doc(hidden)]
pub fn run_case<G, Ctx, F>(app: crate::App<G, Ctx>, args: Vec<String>, check: F)
where
    G: 'static,
    Ctx: 'static,
    F: FnOnce(Result<(G, crate::Invocation<Ctx>), bpaf::ParseFailure>),
{
    check(app.parse(args));
}

/// Convert a value to its argv string, through `Display`.
///
/// No longer used by [`cli!`](crate::cli), which became a proc macro in 0.5
/// and reads each literal itself (ADR-017). Kept, hidden, so code that called
/// it keeps compiling.
#[doc(hidden)]
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
