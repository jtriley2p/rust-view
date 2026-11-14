//! Line classification: code, test, doc, comment, blank.

use crate::report::Lines;
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};

/// What touches a line; the greater kind wins.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    None,
    Doc,
    Code,
}

/// Counts the lines of `text`, whose tokens are `tokens`. A line is code if a
/// token touches it, doc if only doc comments or `#[doc = ..]` attributes do.
/// `tests` are 1-based inclusive line ranges of test code.
pub fn count(text: &str, tokens: TokenStream, tests: &[(usize, usize)]) -> Lines {
    let mut kinds = vec![Kind::None; text.lines().count() + 1];
    mark(tokens, &mut kinds);
    let mut out = Lines::default();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let slot = match kinds[n] {
            Kind::Code if tests.iter().any(|&(a, b)| a <= n && n <= b) => &mut out.test,
            Kind::Code => &mut out.code,
            _ if line.trim().is_empty() => &mut out.blank,
            Kind::Doc => &mut out.doc,
            Kind::None => &mut out.comment,
        };
        *slot += 1;
    }
    out
}

fn mark(tokens: TokenStream, kinds: &mut [Kind]) {
    let mut iter = tokens.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            // `#[doc = ..]` or `#![doc = ..]`. A doc comment lexes as the same, all spanning the comment.
            TokenTree::Punct(p) if p.as_char() == '#' => {
                let bang = iter.next_if(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '!'));
                let attr = iter.next_if(is_doc);
                let kind = if attr.is_some() {
                    Kind::Doc
                } else {
                    Kind::Code
                };
                for t in [Some(TokenTree::Punct(p)), bang, attr]
                    .into_iter()
                    .flatten()
                {
                    touch(t.span(), kind, kinds);
                }
            }
            TokenTree::Group(g) => {
                touch(g.span_open(), Kind::Code, kinds);
                touch(g.span_close(), Kind::Code, kinds);
                mark(g.stream(), kinds);
            }
            other => touch(other.span(), Kind::Code, kinds),
        }
    }
}

/// A `[doc = ..]` group.
fn is_doc(tt: &TokenTree) -> bool {
    let TokenTree::Group(g) = tt else {
        return false;
    };
    let mut inner = g.stream().into_iter();
    g.delimiter() == Delimiter::Bracket
        && matches!(inner.next(), Some(TokenTree::Ident(i)) if i == "doc")
        && matches!(inner.next(), Some(TokenTree::Punct(p)) if p.as_char() == '=')
}

fn touch(span: Span, kind: Kind, kinds: &mut [Kind]) {
    for line in span.start().line..=span.end().line {
        if let Some(k) = kinds.get_mut(line) {
            *k = (*k).max(kind);
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
                doc: 4,
                comment: 3,
                blank: 2
            }
        );
        assert_eq!(
            l.code + l.test + l.doc + l.comment + l.blank,
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
                doc: 1,
                comment: 0,
                blank: 0
            }
        );
    }

    #[test]
    fn doc_attrs() {
        let text = "\
#![doc = include_str!(\"../README.md\")]
#[doc = \"multi

line\"]
#[doc(hidden)]
#[doc = \"inline\"] struct A;
/** block

    doc */
mod m {
    //! inner
    #[test]
    /// in test
    fn t() {}
}
";
        assert_eq!(
            lines(text, &[(12, 14)]),
            Lines {
                code: 4,
                test: 2,
                doc: 7,
                comment: 0,
                blank: 2
            }
        );
    }
}
