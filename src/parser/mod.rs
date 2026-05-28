//! Reads source bytes into an [`Sexp`] tree.
//!
//! The grammar is minimal: a program is a single parenthesized list whose
//! elements are atoms ([`Atomic`]: strings, ints, floats, identifiers) or
//! nested lists. Lists are represented as cons chains terminated by
//! [`Sexp::Unit`] (nil).
//!
//! [`Parser`] streams from any [`Read`] one buffer at a time and tracks a
//! [`Position`] (line/column/byte) so every [`ParseError`] can point at the
//! offending location. Identifiers are interned so repeated names share one
//! `Rc<str>`.

mod error;
mod position;

pub use error::*;
use position::Position;
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read},
};

use std::rc::Rc;

/// A leaf token: a string literal, integer, float, or identifier.
#[derive(Debug, Clone, PartialEq)]
pub enum Atomic {
    Str(Rc<str>),
    Int(i64),
    Float(f64),
    Ident(Rc<str>),
}

/// A parsed s-expression. Lists are cons chains of `Exp { head, tail }` ending
/// in `Unit`, which also stands for the empty list `()` (nil).
#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    Unit,
    Atom(Atomic),
    Exp { head: Rc<Sexp>, tail: Rc<Sexp> },
}

/// A streaming recursive-descent parser over any [`Read`] source.
///
/// `list_depth` tracks open parentheses so an EOF can be reported as a missing
/// close brace; `idents` interns identifier names across the parse.
pub struct Parser<R: Read> {
    reader: BufReader<R>,
    pos: Position,
    list_depth: usize,
    idents: HashSet<Rc<str>>,
}

const WHITESPACE: [u8; 4] = [b'\n', b'\r', b'\t', b' '];
const IDENT_SEPARATORS: [u8; 6] = [b'\n', b'\r', b'\t', b' ', b')', b'('];

impl<R> Parser<R>
where
    R: Read,
{
    /// Creates a parser that reads from `r`.
    pub fn new(r: R) -> Self {
        Self {
            reader: BufReader::new(r),
            pos: Position::new(),
            idents: HashSet::new(),
            list_depth: 0,
        }
    }

    /// Parses a single top-level form, which must be a parenthesized list.
    /// Leading whitespace is skipped; a missing opening `(` is an error.
    pub fn parse(&mut self) -> Result<Sexp, ParseError> {
        self.skip_whitespace()?;
        self.skip_open()?;
        self.parse_list_tail()
    }

    /// The set of identifier names interned so far. Repeated identifiers in the
    /// source share a single `Rc<str>` drawn from this set.
    pub fn idents(&self) -> &HashSet<Rc<str>> {
        &self.idents
    }

    /// Current parser position — points at the next byte to be consumed.
    pub fn position(&self) -> Position {
        self.at()
    }

    fn at(&self) -> Position {
        self.pos
    }

    /// Parses the contents of a list after the opening `(` has been consumed,
    /// up to and including the matching `)`. Returns nil for `()`, otherwise
    /// a cons chain `(head . tail)`.
    fn parse_list_tail(&mut self) -> Result<Sexp, ParseError> {
        self.skip_whitespace()?;
        if self.peek_one()? == b')' {
            self.read_byte()?; // consume ')'
            // Return the nil sentinel. Since parse() returns Sexp (not Rc<Sexp>),
            // we unwrap the Unit variant directly.
            return Ok(Sexp::Unit);
        }

        let head = self.parse_expr()?;
        self.skip_whitespace()?;

        // Tail is either another list element (implicit cons) or `)` (nil terminator).
        let tail = if self.peek_one()? == b')' {
            self.read_byte()?;
            Sexp::Unit
        } else {
            self.parse_list_tail()?
        };

        Ok(Sexp::Exp {
            head: Rc::new(head),
            tail: Rc::new(tail),
        })
    }
    fn parse_expr(&mut self) -> Result<Sexp, ParseError> {
        self.skip_whitespace()?;
        let sexp = match self.peek_one()? {
            b'(' => {
                self.read_byte()?; // consume '('
                self.parse_list_tail()?
            }
            _ => {
                let t = self.parse_atom()?;
                Sexp::Atom(t)
            }
        };
        Ok(sexp)
    }

    fn parse_atom(&mut self) -> Result<Atomic, ParseError> {
        let mut buf = [0u8; 2];
        self.peek_many(&mut buf)?;
        match buf {
            [b'"', _] => self.parse_str(),
            [b'0'..=b'9', _] | [b'-', b'0'..=b'9'] => self.parse_number(),

            _ => self.parse_ident(),
        }
    }

    fn parse_number(&mut self) -> Result<Atomic, ParseError> {
        let mut buf = vec![self.read_byte()?]; // read the first byte in case it's `-`
        self.read_while(&mut buf, |b| b.is_ascii_digit() || *b == b'.')?;

        let at = self.at();
        let s = str::from_utf8(&buf).map_err(|e| ParseError::UTF8Error { source: e, at })?;
        if s.contains('.') {
            let n: f64 = s
                .parse()
                .map_err(|e| ParseError::ParseFloatError { source: e, at })?;
            Ok(Atomic::Float(n))
        } else {
            let n: i64 = s
                .parse()
                .map_err(|e| ParseError::ParseIntError { source: e, at })?;
            Ok(Atomic::Int(n))
        }
    }

    fn parse_ident(&mut self) -> Result<Atomic, ParseError> {
        let mut buf = Vec::new();
        let n = self.read_while(&mut buf, |b| !IDENT_SEPARATORS.contains(b))?;
        assert!(n > 0);

        let at = self.at();
        let s = str::from_utf8(&buf).map_err(|e| ParseError::UTF8Error { source: e, at })?;
        match self.idents.get(s) {
            Some(ident) => Ok(Atomic::Ident(ident.clone())),
            None => {
                let ident: Rc<str> = s.into();
                self.idents.insert(ident.clone());
                Ok(Atomic::Ident(ident.clone()))
            }
        }
    }

    // parses double quoted str including the opening `"` and closing `"`
    // panics if first byte isn't `"`
    fn parse_str(&mut self) -> Result<Atomic, ParseError> {
        let b = self.read_byte()?;
        assert_eq!(b, b'"');

        let mut buf = Vec::new();
        let n = self.read_until(&mut buf, b'"')?;
        if buf.last() != Some(&b'"') {
            // read_until hit EOF before finding the closing quote.
            return Err(ParseError::IOError {
                source: std::io::ErrorKind::UnexpectedEof.into(),
                at: self.at(),
            });
        }
        let inner = &buf[0..n - 1]; // skip closing `"`
        let at = self.at();
        let s: Rc<str> = str::from_utf8(inner)
            .map_err(|e| ParseError::UTF8Error { source: e, at })?
            .into();
        Ok(Atomic::Str(s))
    }

    fn peek_one(&mut self) -> Result<u8, ParseError> {
        let at = self.at();
        let avail = match self.reader.fill_buf() {
            Ok(a) => a,
            Err(e) => return Err(ParseError::IOError { source: e, at }),
        };
        if avail.is_empty() {
            return Err(self.eof_err());
        }
        Ok(avail[0])
    }

    /// Peeks up to `buf.len()` bytes from the reader without consuming them.
    /// On EOF (no bytes available) returns `MissingCloseBrace` or `IOError`
    /// depending on whether we're inside a list. If fewer bytes are available
    /// than `buf.len()`, fills the prefix and leaves the rest of `buf`
    /// untouched.
    fn peek_many(&mut self, buf: &mut [u8]) -> Result<(), ParseError> {
        let at = self.at();
        let avail = match self.reader.fill_buf() {
            Ok(a) => a,
            Err(e) => return Err(ParseError::IOError { source: e, at }),
        };
        if avail.is_empty() {
            return Err(self.eof_err());
        }
        let n = avail.len().min(buf.len());
        buf[..n].copy_from_slice(&avail[..n]);
        Ok(())
    }

    fn eof_err(&self) -> ParseError {
        if self.list_depth > 0 {
            ParseError::MissingCloseBrace { at: self.at() }
        } else {
            ParseError::IOError {
                source: std::io::ErrorKind::UnexpectedEof.into(),
                at: self.at(),
            }
        }
    }

    fn read_until(&mut self, buf: &mut Vec<u8>, byte: u8) -> Result<usize, ParseError> {
        let start = buf.len();
        let at = self.at();
        let n = match self.reader.read_until(byte, buf) {
            Ok(n) => n,
            Err(e) => return Err(ParseError::IOError { source: e, at }),
        };
        self.advance(&buf[start..]);
        Ok(n)
    }

    fn read_byte(&mut self) -> Result<u8, ParseError> {
        let mut buf = [0u8; 1];
        let at = self.at();
        if let Err(e) = self.reader.read_exact(&mut buf) {
            return Err(ParseError::IOError { source: e, at });
        }
        // `at` snapshots the position of the byte we just read; advance afterwards.
        self.advance(&buf);

        match buf[0] {
            b'(' => {
                self.list_depth += 1;
            }
            b')' => {
                self.list_depth = self
                    .list_depth
                    .checked_sub(1)
                    .ok_or(ParseError::UnexpectedCloseBrace { at })?;
            }
            _ => {}
        }

        Ok(buf[0])
    }

    fn skip_open(&mut self) -> Result<(), ParseError> {
        let at = self.at();
        if self.read_byte()? != b'(' {
            Err(ParseError::MissingOpenBrace { at })
        } else {
            Ok(())
        }
    }

    /// Updates `pos`, `line`, `col` to reflect having consumed `bytes`.
    /// `\n` increments `line` and resets `col` to 1; every other byte
    /// increments `col` by 1.
    fn advance(&mut self, bytes: &[u8]) {
        self.pos.advance(bytes);
    }

    fn read_while<P: FnMut(&u8) -> bool>(
        &mut self,
        buf: &mut Vec<u8>,
        mut p: P,
    ) -> Result<usize, ParseError> {
        let start = buf.len();
        let mut read = 0;
        loop {
            let (used, total) = {
                let available = match self.reader.fill_buf() {
                    Ok(b) => b,
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        return Err(ParseError::IOError {
                            source: e,
                            at: self.at(),
                        });
                    }
                };
                if available.is_empty() {
                    break;
                }
                let used = available.iter().take_while(|b| p(b)).count();
                buf.extend_from_slice(&available[..used]);
                (used, available.len())
            };
            self.reader.consume(used);
            read += used;
            if used < total {
                break;
            }
        }
        self.advance(&buf[start..]);
        Ok(read)
    }

    fn skip_whitespace(&mut self) -> Result<(), ParseError> {
        let mut throwaway = Vec::new();
        match self.read_while(&mut throwaway, |b| WHITESPACE.contains(b)) {
            Ok(_) => Ok(()),
            Err(_) => {
                if self.list_depth > 0 {
                    Err(ParseError::UnexpectedCloseBrace { at: self.at() })
                } else {
                    Err(ParseError::IOError {
                        source: std::io::ErrorKind::UnexpectedEof.into(),
                        at: self.at(),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- helpers -----

    fn parse_str(input: &str) -> Result<Sexp, ParseError> {
        Parser::new(input.as_bytes()).parse()
    }

    fn parse_ok(input: &str) -> Sexp {
        parse_str(input).expect("expected successful parse")
    }

    /// Construct a cons-list `Sexp` from a Vec of elements. Mirrors how the
    /// parser builds lists so we can assert on full structures concisely.
    fn list(elems: Vec<Sexp>) -> Sexp {
        let mut result = Sexp::Unit;
        for e in elems.into_iter().rev() {
            result = Sexp::Exp {
                head: Rc::new(e),
                tail: Rc::new(result),
            };
        }
        result
    }

    fn int(n: i64) -> Sexp {
        Sexp::Atom(Atomic::Int(n))
    }
    fn float(f: f64) -> Sexp {
        Sexp::Atom(Atomic::Float(f))
    }
    fn ident(s: &str) -> Sexp {
        Sexp::Atom(Atomic::Ident(s.into()))
    }
    fn string(s: &str) -> Sexp {
        Sexp::Atom(Atomic::Str(s.into()))
    }

    // ----- empty / trivial -----

    #[test]
    fn empty_list_is_unit() {
        assert_eq!(parse_ok("()"), Sexp::Unit);
    }

    #[test]
    fn leading_whitespace_before_open_is_skipped() {
        assert_eq!(parse_ok("   ()"), Sexp::Unit);
        assert_eq!(parse_ok("\n\r ()"), Sexp::Unit);
    }

    #[test]
    fn whitespace_inside_empty_list_is_skipped() {
        assert_eq!(parse_ok("(   )"), Sexp::Unit);
        assert_eq!(parse_ok("(\n)"), Sexp::Unit);
    }

    // ----- atoms inside a single-element list -----

    #[test]
    fn single_int() {
        assert_eq!(parse_ok("(42)"), list(vec![int(42)]));
    }

    #[test]
    fn single_zero() {
        assert_eq!(parse_ok("(0)"), list(vec![int(0)]));
    }

    #[test]
    fn single_float() {
        assert_eq!(parse_ok("(42.069)"), list(vec![float(42.069)]));
    }

    #[test]
    fn float_with_trailing_dot_is_valid() {
        // "1." parses to 1.0 via Rust's f64::from_str
        assert_eq!(parse_ok("(1.)"), list(vec![float(1.0)]));
    }

    #[test]
    fn single_ident() {
        assert_eq!(parse_ok("(foo)"), list(vec![ident("foo")]));
    }

    #[test]
    fn single_string() {
        assert_eq!(parse_ok(r#"("hello")"#), list(vec![string("hello")]));
    }

    #[test]
    fn empty_string_literal() {
        assert_eq!(parse_ok(r#"("")"#), list(vec![string("")]));
    }

    #[test]
    fn string_with_spaces_and_punctuation() {
        assert_eq!(
            parse_ok(r#"("hello, world!")"#),
            list(vec![string("hello, world!")])
        );
    }

    // ----- multi-element lists -----

    #[test]
    fn list_of_ints() {
        assert_eq!(parse_ok("(1 2 3)"), list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn list_of_mixed_atoms() {
        assert_eq!(
            parse_ok(r#"(foo 42 3.5 "bar")"#),
            list(vec![ident("foo"), int(42), float(3.5), string("bar")])
        );
    }

    #[test]
    fn extra_whitespace_between_elements() {
        assert_eq!(
            parse_ok("(  1   2   3  )"),
            list(vec![int(1), int(2), int(3)])
        );
    }

    #[test]
    fn newlines_between_elements() {
        assert_eq!(parse_ok("(1\n2\n3)"), list(vec![int(1), int(2), int(3)]));
    }

    // ----- nested lists -----

    #[test]
    fn nested_list() {
        // (a (b c))  =>  cons(a, cons(cons(b, cons(c, nil)), nil))
        let inner = list(vec![ident("b"), ident("c")]);
        let expected = list(vec![ident("a"), inner]);
        assert_eq!(parse_ok("(a (b c))"), expected);
    }

    #[test]
    fn deeply_nested_list() {
        // (((1)))
        let l1 = list(vec![int(1)]);
        let l2 = list(vec![l1]);
        let l3 = list(vec![l2]);
        assert_eq!(parse_ok("(((1)))"), l3);
    }

    #[test]
    fn nested_empty_lists() {
        // (() ())
        let expected = list(vec![Sexp::Unit, Sexp::Unit]);
        assert_eq!(parse_ok("(() ())"), expected);
    }

    #[test]
    fn list_starting_with_nested_list() {
        // ((a) b)
        let inner = list(vec![ident("a")]);
        let expected = list(vec![inner, ident("b")]);
        assert_eq!(parse_ok("((a) b)"), expected);
    }

    // ----- identifier interning -----

    #[test]
    fn repeated_identifiers_share_rc() {
        let parsed = parse_ok("(foo foo)");
        // Walk the structure and pull out the two Rc<str> values.
        let (a, b) = match parsed {
            Sexp::Exp { head, tail } => {
                let a = match &*head {
                    Sexp::Atom(Atomic::Ident(s)) => s.clone(),
                    other => panic!("expected ident, got {:?}", other),
                };
                let b = match &*tail {
                    Sexp::Exp { head, .. } => match &**head {
                        Sexp::Atom(Atomic::Ident(s)) => s.clone(),
                        other => panic!("expected ident, got {:?}", other),
                    },
                    other => panic!("expected cons, got {:?}", other),
                };
                (a, b)
            }
            other => panic!("expected non-empty list, got {:?}", other),
        };
        assert!(
            Rc::ptr_eq(&a, &b),
            "expected the same Rc<str> for repeated identifiers"
        );
    }

    #[test]
    fn distinct_identifiers_do_not_share_rc() {
        let parsed = parse_ok("(foo bar)");
        let (a, b) = match parsed {
            Sexp::Exp { head, tail } => {
                let a = match &*head {
                    Sexp::Atom(Atomic::Ident(s)) => s.clone(),
                    _ => panic!(),
                };
                let b = match &*tail {
                    Sexp::Exp { head, .. } => match &**head {
                        Sexp::Atom(Atomic::Ident(s)) => s.clone(),
                        _ => panic!(),
                    },
                    _ => panic!(),
                };
                (a, b)
            }
            _ => panic!(),
        };
        assert!(!Rc::ptr_eq(&a, &b));
        assert_ne!(&*a, &*b);
    }

    // ----- numbers -----

    #[test]
    fn large_positive_int() {
        assert_eq!(parse_ok("(9223372036854775807)"), list(vec![int(i64::MAX)]));
    }

    #[test]
    fn int_overflow_is_error() {
        // i64::MAX + 1
        let err = parse_str("(9223372036854775808)").unwrap_err();
        assert!(
            matches!(err, ParseError::ParseIntError { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn malformed_float_is_error() {
        // Two dots: read_while collects "1.2.3", parse::<f64>() fails.
        let err = parse_str("(1.2.3)").unwrap_err();
        assert!(
            matches!(err, ParseError::ParseFloatError { .. }),
            "got {:?}",
            err
        );
    }

    // ----- identifiers: lexer boundaries -----

    #[test]
    fn ident_with_internal_punctuation_is_one_token() {
        // parse_ident stops only on space or ')'. A leading '-' now dispatches
        // to parse_number, but '-' inside an identifier (after a non-'-' first
        // byte) is fine.
        assert_eq!(parse_ok("(foo-bar)"), list(vec![ident("foo-bar")]));
        assert_eq!(parse_ok("(<=)"), list(vec![ident("<=")]));
        assert_eq!(parse_ok("(+)"), list(vec![ident("+")]));
    }

    #[test]
    fn negative_int() {
        assert_eq!(parse_ok("(-42)"), list(vec![int(-42)]));
    }

    #[test]
    fn negative_float() {
        assert_eq!(parse_ok("(-42.069)"), list(vec![float(-42.069)]));
    }

    #[test]
    fn negative_zero_int() {
        // -0 as an integer is just 0; this verifies the negation path doesn't crash.
        assert_eq!(parse_ok("(-0)"), list(vec![int(0)]));
    }

    // ----- error cases -----

    #[test]
    fn missing_open_paren_is_error() {
        let err = parse_str("foo)").unwrap_err();
        assert!(
            matches!(err, ParseError::MissingOpenBrace { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn empty_input_is_error() {
        let err = parse_str("").unwrap_err();
        // skip_whitespace -> skip_open -> read_byte -> UnexpectedEof
        assert!(matches!(err, ParseError::IOError { .. }), "got {:?}", err);
    }

    #[test]
    fn unterminated_list_is_missing_close_brace_error() {
        let err = parse_str("(1 2").unwrap_err();
        assert!(
            matches!(err, ParseError::MissingCloseBrace { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn unterminated_string_then_eof_errors() {
        let err = parse_str(r#"("abc"#).unwrap_err();
        assert!(matches!(err, ParseError::IOError { .. }), "got {:?}", err);
    }

    // ----- pos tracking on error -----

    #[test]
    fn missing_open_brace_reports_position() {
        let err = parse_str("x").unwrap_err();
        match err {
            ParseError::MissingOpenBrace { at } => {
                assert_eq!(at.byte, 0);
                assert_eq!(at.line, 1);
                assert_eq!(at.col, 1);
            }
            other => panic!("expected MissingOpenBrace, got {:?}", other),
        }
    }

    #[test]
    fn missing_open_brace_position_accounts_for_skipped_whitespace() {
        // skip_whitespace consumes three spaces (byte = 3), then skip_open
        // snapshots position before reading 'x'.
        let err = parse_str("   x").unwrap_err();
        match err {
            ParseError::MissingOpenBrace { at } => {
                assert_eq!(at.byte, 3);
                assert_eq!(at.line, 1);
                assert_eq!(at.col, 4);
            }
            other => panic!("expected MissingOpenBrace, got {:?}", other),
        }
    }

    // ----- line/column tracking -----

    #[test]
    fn position_tracks_lines_after_newlines() {
        // After parsing "(a\nb)" the parser sits just past ')':
        // byte=5, line=2, col=3
        let mut p = Parser::new("(a\nb)".as_bytes());
        p.parse().unwrap();
        let pos = p.position();
        assert_eq!(pos.byte, 5);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.col, 3);
    }

    #[test]
    fn missing_open_brace_reports_line_and_col() {
        // Two newlines, two spaces, then 'x' — 'x' is at line 3, col 3.
        let err = parse_str("\n\n  x").unwrap_err();
        match err {
            ParseError::MissingOpenBrace { at } => {
                assert_eq!(at.byte, 4);
                assert_eq!(at.line, 3);
                assert_eq!(at.col, 3);
            }
            other => panic!("expected MissingOpenBrace, got {:?}", other),
        }
    }

    #[test]
    fn unexpected_close_brace_reports_line_and_col() {
        // Newline then ')' at top-level — ')' is at line 2, col 1.
        let err = parse_str("\n)").unwrap_err();
        match err {
            ParseError::UnexpectedCloseBrace { at } => {
                assert_eq!(at.byte, 1);
                assert_eq!(at.line, 2);
                assert_eq!(at.col, 1);
            }
            other => panic!("expected UnexpectedCloseBrace, got {:?}", other),
        }
    }

    #[test]
    fn missing_close_brace_reports_line_and_col() {
        // Unterminated list across two lines — EOF reached at line 2, col 3.
        let err = parse_str("(1\n2").unwrap_err();
        match err {
            ParseError::MissingCloseBrace { at } => {
                assert_eq!(at.line, 2);
                assert_eq!(at.col, 2);
            }
            other => panic!("expected MissingCloseBrace, got {:?}", other),
        }
    }

    #[test]
    fn parse_int_error_reports_line_and_col() {
        // Overflow on line 2.
        let err = parse_str("(\n9223372036854775808)").unwrap_err();
        match err {
            ParseError::ParseIntError { at, .. } => {
                assert_eq!(at.line, 2);
            }
            other => panic!("expected ParseIntError, got {:?}", other),
        }
    }

    // ----- Read impl: works on any Read, not just &[u8] -----

    #[test]
    fn parses_from_cursor() {
        use std::io::Cursor;
        let mut p = Parser::new(Cursor::new(b"(1 2)".to_vec()));
        assert_eq!(p.parse().unwrap(), list(vec![int(1), int(2)]));
    }

    // ----- realistic-ish input -----

    #[test]
    fn realistic_program_like_input() {
        // (defn square (x) (* x x))
        let inner_args = list(vec![ident("x")]);
        let inner_body = list(vec![ident("*"), ident("x"), ident("x")]);
        let expected = list(vec![ident("defn"), ident("square"), inner_args, inner_body]);
        assert_eq!(parse_ok("(defn square (x) (* x x))"), expected);
    }
}
