//! Line classification: code, test, comment, blank.

use crate::report::Lines;
use proc_macro2::{Span, TokenStream, TokenTree};

/// Counts the lines of `text`, whose tokens are `tokens`. A line is code if a
/// token touches it. `tests` are 1-based inclusive line ranges of test code.
pub fn count(text: &str, tokens: TokenStream, tests: &[(usize, usize)]) -> Lines {
    let mut code = vec![false; text.lines().count() + 1];
    mark(tokens, &mut code);
    let mut out = Lines::default();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let slot = if !code[n] {
            if line.trim().is_empty() {
                &mut out.blank
            } else {
                &mut out.comment
            }
        } else if tests.iter().any(|&(a, b)| a <= n && n <= b) {
            &mut out.test
        } else {
            &mut out.code
        };
        *slot += 1;
    }
    out
}

fn mark(tokens: TokenStream, code: &mut [bool]) {
    let mut iter = tokens.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            // A doc comment lexes as `#[doc = ".."]` or `#![doc = ".."]`, all spanning the comment.
            TokenTree::Punct(p) if p.as_char() == '#' && is_comment(p.span()) => {
                iter.next_if(|t| matches!(t, TokenTree::Punct(_)));
                iter.next();
            }
            TokenTree::Group(g) => {
                touch(g.span_open(), code);
                touch(g.span_close(), code);
                mark(g.stream(), code);
            }
            other => touch(other.span(), code),
        }
    }
}

fn is_comment(span: Span) -> bool {
    span.source_text().is_some_and(|s| s.starts_with('/'))
}

fn touch(span: Span, code: &mut [bool]) {
    for line in span.start().line..=span.end().line {
        if let Some(c) = code.get_mut(line) {
            *c = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str, tests: &[(usize, usize)]) -> Lines {
        count(text, text.parse().unwrap(), tests)
    }

    #[test]
    fn classifies() {
        let text = "\
//! inner doc
// plain

/// outer doc
/** block
    doc */
#[derive(Debug)]
struct A; // trailing
/* block

   comment */
const S: &str = \"multi

line\";
fn f() {
}
";
        let l = lines(text, &[(15, 16)]);
        assert_eq!(
            l,
            Lines {
                code: 5,
                test: 2,
                comment: 7,
                blank: 2
            }
        );
        assert_eq!(
            l.code + l.test + l.comment + l.blank,
            text.lines().count() as u32
        );
    }

    #[test]
    fn unicode_and_division() {
        let l = lines(
            "fn f() -> f64 { \"é\".len() as f64 / 2.0 } /// é\n/// é\n",
            &[],
        );
        assert_eq!(
            l,
            Lines {
                code: 1,
                test: 0,
                comment: 1,
                blank: 0
            }
        );
    }
}
