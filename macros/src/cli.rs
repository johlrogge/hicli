//! `cli!{…}`: raw command-line tokens into an argv `Vec<String>`.
//!
//! One rule decides everything: **tokens that touch in the source are one
//! word**, and whitespace separates words — the shell's rule. `set-output` is
//! three tokens to the Rust lexer (`set`, `-`, `output`) but they touch, so it
//! is one word; `a - b` is three words because there is space between them.
//!
//! This is why it is a proc macro and not `macro_rules!`: a declarative macro
//! sees neither whitespace nor spans, so `-x` and `- x` look the same to it.
//!
//! Whether two tokens touch is read from their line/column locations
//! (`prev.end() == next.start()`), which proc-macro2's `span-locations`
//! feature exposes on stable. `Span::join` would be the other way to ask, and
//! it needs the nightly-only `proc_macro_span`, so it is not used (ADR-017).
//!
//! A real token is never zero-width, so a span with `start == end` means the
//! front end gave no location (rust-analyzer's proc-macro server may do this).
//! Such a token is never joined to its neighbour: without locations every
//! token is its own word, which is the old token-per-word reading.

use proc_macro2::{Delimiter, LineColumn, Span, TokenStream, TokenTree};
use quote::quote;
use syn::{Error, Lit, Result};

/// The message for `()`, `[]` or `{}` inside `cli!`.
pub(crate) const BRACKETED_GROUP: &str = "cli!: a bracketed group is not a word; quote it: \"[a]\"";

/// The message for a byte string or a C string inside `cli!`.
pub(crate) const BYTE_STRING: &str =
    "cli!: a byte string is not a word; write a \"string literal\"";

/// Expand the contents of `cli!{…}`.
pub(crate) fn expand(input: TokenStream) -> TokenStream {
    match words(input) {
        Ok(words) => quote! {
            {
                let __v: ::std::vec::Vec<::std::string::String> =
                    ::std::vec![ #(::std::string::String::from(#words)),* ];
                __v
            }
        },
        Err(e) => e.to_compile_error(),
    }
}

/// The argv words `input` denotes.
pub(crate) fn words(input: TokenStream) -> Result<Vec<String>> {
    let mut pieces = Vec::new();
    flatten(input, &mut pieces)?;

    let mut words: Vec<String> = Vec::new();
    let mut prev: Option<&Piece> = None;
    for piece in &pieces {
        match (prev, words.last_mut()) {
            (Some(p), Some(word)) if touches(p, piece) => word.push_str(&piece.text),
            _ => words.push(piece.text.clone()),
        }
        prev = Some(piece);
    }
    Ok(words)
}

/// One top-level token: its text as a part of a word, and where it is.
struct Piece {
    text: String,
    start: LineColumn,
    end: LineColumn,
}

impl Piece {
    fn new(text: String, span: Span) -> Self {
        Piece {
            text,
            start: span.start(),
            end: span.end(),
        }
    }

    /// Whether this token carries a real location — see the module docs.
    fn located(&self) -> bool {
        self.start != self.end
    }
}

/// Whether `next` starts exactly where `prev` ends, with no space between.
fn touches(prev: &Piece, next: &Piece) -> bool {
    prev.located() && next.located() && prev.end == next.start
}

/// The tokens of `input`, invisible groups flattened in place.
fn flatten(input: TokenStream, out: &mut Vec<Piece>) -> Result<()> {
    for tree in input {
        match tree {
            // What a `macro_rules!` forwarder wraps an `:expr` in: not a
            // bracket anyone wrote, so its tokens stand where it stood.
            TokenTree::Group(g) if g.delimiter() == Delimiter::None => flatten(g.stream(), out)?,
            TokenTree::Group(g) => return Err(Error::new(g.span(), BRACKETED_GROUP)),
            TokenTree::Ident(i) => out.push(Piece::new(i.to_string(), i.span())),
            TokenTree::Punct(p) => out.push(Piece::new(p.as_char().to_string(), p.span())),
            TokenTree::Literal(l) => {
                let span = l.span();
                let text = match Lit::new(l.clone()) {
                    Lit::Str(s) => s.value(),
                    Lit::Char(c) => c.value().to_string(),
                    Lit::ByteStr(_) | Lit::CStr(_) => return Err(Error::new(span, BYTE_STRING)),
                    // Numbers keep their spelling: `0x10` is `0x10`, not `16`.
                    _ => l.to_string(),
                };
                out.push(Piece::new(text, span));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Group;
    use std::str::FromStr;

    /// The words of `src`, lexed with real (fallback) locations.
    fn w(src: &str) -> Vec<String> {
        words(TokenStream::from_str(src).expect("lexes")).expect("is words")
    }

    /// The error `src` is rejected with.
    fn err(src: &str) -> String {
        words(TokenStream::from_str(src).expect("lexes"))
            .expect_err("is rejected")
            .to_string()
    }

    #[test]
    fn the_mapping_table() {
        let rows: &[(&str, &[&str])] = &[
            (
                "playback set-output hdmi",
                &["playback", "set-output", "hdmi"],
            ),
            ("--dry-run", &["--dry-run"]),
            ("-v", &["-v"]),
            ("-5", &["-5"]),
            ("--since=-7", &["--since=-7"]),
            ("--name=\"My Event\"", &["--name=My Event"]),
            ("email,phone", &["email,phone"]),
            ("a/b/c", &["a/b/c"]),
            ("./ev", &["./ev"]),
            ("-- foo", &["--", "foo"]),
            ("--", &["--"]),
            ("a - b", &["a", "-", "b"]),
            ("- 5", &["-", "5"]),
            ("\"\"", &[""]),
            ("x \"My Event\"", &["x", "My Event"]),
            ("\"a\"\"b\"", &["ab"]),
            ("", &[]),
        ];
        for (src, want) in rows {
            assert_eq!(w(src), *want, "cli! {{ {src} }}");
        }
    }

    #[test]
    fn literals_are_kept_as_written() {
        assert_eq!(w("0x10 1.50 5u8"), ["0x10", "1.50", "5u8"]);
        assert_eq!(w("-0x10"), ["-0x10"]);
    }

    #[test]
    fn strings_and_chars_are_their_values() {
        assert_eq!(w(r##"r#"a "b""# 'c' "\t""##), ["a \"b\"", "c", "\t"]);
    }

    #[test]
    fn a_new_line_separates_words() {
        assert_eq!(w("a\nb"), ["a", "b"]);
        assert_eq!(w("set-\noutput"), ["set-", "output"]);
    }

    #[test]
    fn an_invisible_group_is_flattened_in_place() {
        // What `macro_rules!` hands on for an `:expr` capture: the tokens keep
        // their own spans, so they join their neighbours exactly as written.
        let toks: Vec<TokenTree> = TokenStream::from_str("a --x \"My Event\"")
            .expect("lexes")
            .into_iter()
            .collect();
        let wrap = |t: &[TokenTree]| {
            TokenTree::Group(Group::new(Delimiter::None, t.iter().cloned().collect()))
        };
        // `a`, `-`, `-`, `x`, `"My Event"`: wrap `x` alone, and the string alone.
        let mut stream: Vec<TokenTree> = toks[..3].to_vec();
        stream.push(wrap(&toks[3..4]));
        stream.push(wrap(&toks[4..5]));
        let got = words(stream.into_iter().collect()).expect("is words");
        assert_eq!(got, ["a", "--x", "My Event"]);
    }

    #[test]
    fn tokens_without_locations_are_separate_words() {
        // `quote!` tokens are spanned at the call site, which in the fallback
        // has no width: the guard keeps them apart, as before #22.
        let got = words(quote!(set - output)).expect("is words");
        assert_eq!(got, ["set", "-", "output"]);
    }

    #[test]
    fn a_bracketed_group_is_rejected() {
        for src in ["[a]", "x (a)", "{a}"] {
            assert_eq!(err(src), BRACKETED_GROUP, "{src}");
        }
    }

    #[test]
    fn a_byte_string_is_rejected() {
        for src in ["b\"a\"", "br\"a\"", "c\"a\""] {
            assert_eq!(err(src), BYTE_STRING, "{src}");
        }
    }

    #[test]
    fn the_expansion_is_a_typed_vec() {
        let got = expand(TokenStream::from_str("set-output").expect("lexes")).to_string();
        let want = quote! {
            {
                let __v: ::std::vec::Vec<::std::string::String> =
                    ::std::vec![ ::std::string::String::from("set-output") ];
                __v
            }
        };
        assert_eq!(got, want.to_string());
    }
}
