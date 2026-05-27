use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read},
};

use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("expected open brace at byte {pos}")]
    MissingOpenBrace { pos: usize },
    #[error("expected close brace at byte {pos}")]
    MissingCloseBrace { pos: usize },
    #[error("unexpected close brace at byte {pos}")]
    UnexpectedCloseBrace { pos: usize },

    #[error("str not utf-8")]
    UTF8Error(#[from] std::str::Utf8Error),
    #[error("string not utf-8")]
    FromUTF8Error(#[from] std::string::FromUtf8Error),

    #[error("parse float error")]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error("parse int error")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("io error encountered during parsing: {0}")]
    IOError(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Atomic {
    Str(Rc<str>),
    Int(i64),
    Float(f64),
    Ident(Rc<str>),
}
#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    Unit,
    Atom(Atomic),
    Exp { head: Rc<Sexp>, tail: Rc<Sexp> },
}

pub struct Parser<R: Read> {
    reader: BufReader<R>,
    pos: usize,
    list_depth: usize,
    sexp_nil: Rc<Sexp>,
    idents: HashSet<Rc<str>>,
}

const WHITESPACE: [u8; 4] = [b'\n', b'\r', b'\t', b' '];
const IDENT_SEPARATORS: [u8; 6] = [b'\n', b'\r', b'\t', b' ', b')', b'('];

impl<R> Parser<R>
where
    R: Read,
{
    pub fn new(r: R) -> Self {
        Self {
            reader: BufReader::new(r),
            pos: 0,
            sexp_nil: Rc::new(Sexp::Unit),
            idents: HashSet::new(),
            list_depth: 0,
        }
    }
    pub fn parse(&mut self) -> Result<Sexp, ParseError> {
        self.skip_whitespace()?;
        self.skip_open()?;
        self.parse_list_tail()
    }

    pub fn idents(&self) -> &HashSet<Rc<str>> {
        &self.idents
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
            self.sexp_nil.clone()
        } else {
            Rc::new(self.parse_list_tail()?)
        };

        Ok(Sexp::Exp { head, tail })
    }
    fn parse_expr(&mut self) -> Result<Rc<Sexp>, ParseError> {
        self.skip_whitespace()?;
        let sexp = match self.peek_one()? {
            b'(' => {
                self.read_byte()?; // consume '('
                Rc::new(self.parse_list_tail()?)
            }
            _ => {
                let t = self.parse_atom()?;
                Rc::new(Sexp::Atom(t))
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

        let s = str::from_utf8(&buf)?;
        if s.contains('.') {
            let n: f64 = s.parse()?;
            Ok(Atomic::Float(n))
        } else {
            let n: i64 = s.parse()?;
            Ok(Atomic::Int(n))
        }
    }

    fn parse_ident(&mut self) -> Result<Atomic, ParseError> {
        let mut buf = Vec::new();
        let n = self.read_while(&mut buf, |b| !IDENT_SEPARATORS.contains(b))?;
        assert!(n > 0);

        let s = str::from_utf8(&buf)?;
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
            return Err(ParseError::IOError(
                std::io::ErrorKind::UnexpectedEof.into(),
            ));
        }
        let buf = &buf[0..n - 1]; // skip closing `"`
        let s = str::from_utf8(buf)?.into();
        Ok(Atomic::Str(s))
    }

    fn peek_one(&mut self) -> Result<u8, ParseError> {
        let avail = self.reader.fill_buf()?;
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
        let avail = self.reader.fill_buf()?;
        if avail.is_empty() {
            return Err(self.eof_err());
        }
        let n = avail.len().min(buf.len());
        buf[..n].copy_from_slice(&avail[..n]);
        Ok(())
    }

    fn eof_err(&self) -> ParseError {
        if self.list_depth > 0 {
            ParseError::MissingCloseBrace { pos: self.pos }
        } else {
            ParseError::IOError(std::io::ErrorKind::UnexpectedEof.into())
        }
    }

    fn read_until(&mut self, buf: &mut Vec<u8>, byte: u8) -> Result<usize, ParseError> {
        let n = self.reader.read_until(byte, buf)?;
        self.pos += n;
        Ok(n)
    }
    fn read_byte(&mut self) -> Result<u8, ParseError> {
        let mut buf = [0u8; 1];
        self.reader.read_exact(&mut buf)?;
        self.pos += 1;

        match buf[0] {
            b'(' => {
                self.list_depth += 1;
            }
            b')' => {
                self.list_depth
                    .checked_sub(1)
                    .ok_or(ParseError::UnexpectedCloseBrace { pos: self.pos - 1 })?;
            }
            _ => {}
        }

        Ok(buf[0])
    }

    fn skip_open(&mut self) -> Result<(), ParseError> {
        if self.read_byte()? != b'(' {
            Err(ParseError::MissingOpenBrace { pos: self.pos - 1 })
        } else {
            Ok(())
        }
    }

    fn read_while<P: FnMut(&u8) -> bool>(
        &mut self,
        buf: &mut Vec<u8>,
        p: P,
    ) -> Result<usize, ParseError> {
        let n = read_while(&mut self.reader, buf, p)?;
        self.pos += n;
        Ok(n)
    }

    fn skip_whitespace(&mut self) -> Result<(), ParseError> {
        match skip_while(&mut self.reader, |p| WHITESPACE.contains(p)) {
            Ok(n) => {
                self.pos += n;
                Ok(())
            }
            Err(_) => {
                if self.list_depth > 0 {
                    Err(ParseError::UnexpectedCloseBrace { pos: self.pos })
                } else {
                    Err(ParseError::IOError(
                        std::io::ErrorKind::UnexpectedEof.into(),
                    ))
                }
            }
        }
    }
}

fn skip_while<R: BufRead + ?Sized, P: FnMut(&u8) -> bool>(
    r: &mut R,
    mut p: P,
) -> std::io::Result<usize> {
    let mut read = 0;
    loop {
        let (used, total) = {
            let available = match r.fill_buf() {
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if available.is_empty() {
                return Ok(read); // EOF
            }
            let used = available.iter().take_while(|b| p(b)).count();
            (used, available.len())
        };
        r.consume(used);
        read += used;
        if used < total {
            return Ok(read);
        }
    }
}

fn read_while<R: BufRead + ?Sized, P: FnMut(&u8) -> bool>(
    r: &mut R,
    buf: &mut Vec<u8>,
    mut p: P,
) -> std::io::Result<usize> {
    let mut read = 0;
    loop {
        let (used, total) = {
            let available = match r.fill_buf() {
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if available.is_empty() {
                return Ok(read); // EOF
            }
            let used = available.iter().take_while(|b| p(b)).count();
            buf.extend_from_slice(&available[..used]);
            (used, available.len())
        };
        r.consume(used);
        read += used;
        if used < total {
            return Ok(read);
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
        assert!(matches!(err, ParseError::ParseIntError(_)), "got {:?}", err);
    }

    #[test]
    fn malformed_float_is_error() {
        // Two dots: read_while collects "1.2.3", parse::<f64>() fails.
        let err = parse_str("(1.2.3)").unwrap_err();
        assert!(
            matches!(err, ParseError::ParseFloatError(_)),
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
        assert!(matches!(err, ParseError::IOError(_)), "got {:?}", err);
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
        assert!(matches!(err, ParseError::IOError(_)), "got {:?}", err);
    }

    // ----- pos tracking on error -----

    #[test]
    fn missing_open_brace_reports_position() {
        let err = parse_str("x").unwrap_err();
        match err {
            ParseError::MissingOpenBrace { pos } => assert_eq!(pos, 0),
            other => panic!("expected MissingOpenBrace, got {:?}", other),
        }
    }

    #[test]
    fn missing_open_brace_position_accounts_for_skipped_whitespace() {
        // skip_whitespace consumes three spaces (pos = 3), then read_byte
        // reads 'x' (pos = 4), then skip_open reports pos - 1 = 3.
        let err = parse_str("   x").unwrap_err();
        match err {
            ParseError::MissingOpenBrace { pos } => assert_eq!(pos, 3),
            other => panic!("expected MissingOpenBrace, got {:?}", other),
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
