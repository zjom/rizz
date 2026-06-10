//! Reads source bytes into a sequence of [`Sexp`] forms.
//!
//! The grammar is minimal: a program is a sequence of one-or-more top-level
//! forms — each form is an atom ([`Atomic`]: strings, ints, floats,
//! identifiers), a parenthesized list, a collection, or a reader-macro form
//! (`'`, `` ` ``, `,`, `,@`). Lists are represented as cons chains terminated
//! by [`Sexp::Unit`] (nil). Multiple top-level forms are implicitly sequenced
//! and evaluated in order, sharing one threaded environment.
//!
//! [`Parser`] streams from any [`Read`] one buffer at a time and tracks a
//! [`Position`] (line/column/byte) so every [`ParseError`] can point at the
//! offending location. Identifiers are interned so repeated names share one
//! `Rc<str>`.

mod error;
mod position;
pub use error::*;
use im::{HashMap, Vector};
use ordered_float::OrderedFloat;
pub use position::Position;
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read},
};

use std::rc::Rc;

/// A leaf token: a string literal, integer, float, or identifier.
///
/// Identifiers are interned across the parse — equal names share one
/// `Rc<str>` allocation — so comparing or hashing them is `Rc`-fast.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Atomic {
    /// A UTF-8 string literal with `\"`, `\\`, `\n`, `\r`, `\t` escapes.
    Str(Rc<str>),
    /// A 64-bit signed integer. Overflow at parse time is an error.
    Int(i64),
    /// A 64-bit IEEE-754 float. `1.` parses as `1.0`.
    Float(OrderedFloat<f64>),
    /// An identifier — anything that isn't a delimiter, comment, number,
    /// or string. Punctuation like `+`, `<=`, `set!` are valid identifiers.
    Ident(Rc<str>),
}

/// A parsed s-expression. Lists are cons chains of `Exp { head, tail }`
/// ending in `Unit`, which also stands for the empty list `()` (nil).
///
/// `Sexp` is the parser's output type and the input the runtime lowers
/// into [`crate::runtime::Value`] (via the `From<Sexp> for Value` impl)
/// before evaluating. Use it directly when you want tooling that only
/// needs the AST — formatters, linters, source rewriters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sexp {
    /// The empty list / nil sentinel.
    Unit,
    /// An atom: int, float, string, or identifier.
    Atom(Atomic),
    /// A cons cell `(head . tail)`. Proper lists terminate in `Unit`;
    /// improper (dotted) lists end in some other `Sexp`.
    Exp { head: Rc<Sexp>, tail: Rc<Sexp> },
    /// An array `[...]` or map `{...}` literal.
    Collection(Collection),
}

/// A bracketed collection literal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Collection {
    /// `[a b c]` — a persistent vector of forms.
    Array(Vector<Rc<Sexp>>),
    /// `{k: v, ...}` — a persistent map keyed by any `Sexp`.
    Map(HashMap<Rc<Sexp>, Rc<Sexp>>),
}

impl Drop for Sexp {
    /// Unlinks the cons spine iteratively so that dropping a long list does
    /// not recurse once per element (the derived drop would overflow the
    /// stack on lists tens of thousands of elements long). Nested *heads*
    /// still drop recursively, but their depth is bounded by the parser's
    /// nesting limit.
    fn drop(&mut self) {
        let Sexp::Exp { tail, .. } = self else {
            return;
        };
        let mut cur = std::mem::replace(tail, Rc::new(Sexp::Unit));
        // Each owned node has its tail snipped before it drops, so its own
        // `drop` terminates immediately. A shared tail (`Err`) is left for
        // its remaining owners.
        while let Ok(mut node) = Rc::try_unwrap(cur) {
            match &mut node {
                Sexp::Exp { tail, .. } => cur = std::mem::replace(tail, Rc::new(Sexp::Unit)),
                _ => break,
            }
        }
    }
}

/// A streaming recursive-descent parser over any [`Read`] source.
///
/// Reads one buffer at a time, so the source can be a file, a network
/// stream, or anything else implementing [`Read`] — there's no need to
/// slurp the entire input into memory first. Identifier names are
/// interned via a private `HashSet`, so repeated names share one
/// `Rc<str>` for the lifetime of the parse.
pub struct Parser<R: Read> {
    reader: BufReader<R>,
    pos: Position,
    list_depth: usize,
    expr_depth: usize,
    idents: HashSet<Rc<str>>,
}

const WHITESPACE: &[u8] = b"\n\r\t ";
const IDENT_SEPARATORS: &[u8] = b"\n\r\t ;()[]{}:";

/// Maximum nesting depth of forms. Each list/array/map level recurses once
/// in `parse_expr`; this cap turns pathological inputs (e.g. a million `(`s)
/// into a [`ParseError::TooDeep`] instead of a stack overflow. Sized for
/// the smallest common stack (2 MiB thread default) in debug builds, where
/// each nesting level costs a few KiB of stack.
const MAX_NESTING_DEPTH: usize = 256;

impl<R> Parser<R>
where
    R: Read,
{
    /// A new parser that reads from `r`. The source is buffered
    /// internally, so passing an unbuffered `Read` is fine.
    pub fn new(r: R) -> Self {
        Self {
            reader: BufReader::new(r),
            pos: Position::new(),
            idents: HashSet::new(),
            list_depth: 0,
            expr_depth: 0,
        }
    }

    /// Parse every top-level form to EOF.
    ///
    /// Each form is an atom, list, collection (`[...]` / `{...}`), or
    /// reader-macro form (`'X`, `` `X ``, `,X`, `,@X`). Whitespace and `;;`
    /// line comments between forms are skipped. Empty (or comment-only)
    /// input is a [`ParseError::UnexpectedEof`]; otherwise the returned
    /// `Vec` is non-empty and in source order, ready for the runtime to
    /// evaluate as an implicitly sequenced program.
    ///
    /// # Examples
    ///
    /// ```
    /// use rizz::Parser;
    ///
    /// let forms = Parser::new(b"(let x 1) (+ x 2)".as_ref()).parse().unwrap();
    /// assert_eq!(forms.len(), 2);
    /// ```
    pub fn parse(&mut self) -> Result<Vec<Sexp>, ParseError> {
        let mut forms = Vec::new();
        self.skip_trivia()?;
        if self.peek_eof()?.is_none() {
            return Err(ParseError::UnexpectedEof { at: self.at() });
        }
        loop {
            forms.push(self.parse_expr()?);
            self.skip_trivia()?;
            if self.peek_eof()?.is_none() {
                return Ok(forms);
            }
        }
    }

    /// The set of identifier names interned so far. Repeated identifiers
    /// in the source share a single `Rc<str>` drawn from this set —
    /// useful for tools that want to enumerate all the names used in a
    /// program after a parse.
    pub fn idents(&self) -> &HashSet<Rc<str>> {
        &self.idents
    }

    /// Current parser position — points at the next byte to be consumed.
    /// Useful for error reporting beyond what [`ParseError`] already
    /// carries (e.g. printing a caret in the source).
    pub fn position(&self) -> Position {
        self.at()
    }

    fn at(&self) -> Position {
        self.pos
    }

    /// Parses the contents of a list after the opening `(` has been consumed,
    /// up to and including the matching `)`. Returns nil for `()`, otherwise
    /// a cons chain `(head . tail)`. An explicit `.` between elements introduces
    /// a dotted (improper) list whose final tail is the form after the dot, so
    /// `(a b . c)` parses to `Cons(a, Cons(b, c))` rather than terminating in
    /// `Unit`.
    ///
    /// Elements are collected iteratively (not by recursing per element), so
    /// list *length* never grows the stack — only nesting depth does, and
    /// that is bounded by [`MAX_NESTING_DEPTH`].
    fn parse_list_tail(&mut self) -> Result<Sexp, ParseError> {
        let mut elems: Vec<Sexp> = Vec::new();
        let mut last_tail = Sexp::Unit;
        loop {
            self.skip_trivia()?;
            if self.peek_one()? == b')' {
                self.read_byte()?;
                break;
            }

            elems.push(self.parse_expr()?);
            self.skip_trivia()?;

            if self.peek_dotted_marker()? {
                self.read_byte()?;
                self.skip_trivia()?;
                last_tail = self.parse_expr()?;
                self.skip_trivia()?;
                let at = self.at();
                match self.read_byte()? {
                    b')' => break,
                    other => {
                        return Err(ParseError::ExpectedToken {
                            expected: ')',
                            at,
                            got: other.into(),
                        });
                    }
                }
            }
        }

        Ok(elems.into_iter().rfold(last_tail, |tail, head| Sexp::Exp {
            head: Rc::new(head),
            tail: Rc::new(tail),
        }))
    }

    /// A `.` between list elements introduces a dotted tail iff it stands
    /// alone — i.e. is immediately followed by whitespace or `)`. Floats
    /// (`1.5`) and identifiers that contain `.` (`foo.bar`) never trigger this
    /// path because they are consumed whole by `parse_expr` before the next
    /// inter-element peek.
    fn peek_dotted_marker(&mut self) -> Result<bool, ParseError> {
        let [a, b] = self.peek_two()?;
        Ok(a == b'.' && (WHITESPACE.contains(&b) || b == b')'))
    }
    fn parse_expr(&mut self) -> Result<Sexp, ParseError> {
        self.expr_depth += 1;
        if self.expr_depth > MAX_NESTING_DEPTH {
            self.expr_depth -= 1;
            return Err(ParseError::TooDeep {
                at: self.at(),
                limit: MAX_NESTING_DEPTH,
            });
        }
        let result = self.parse_expr_inner();
        self.expr_depth -= 1;
        result
    }

    fn parse_expr_inner(&mut self) -> Result<Sexp, ParseError> {
        self.skip_trivia()?;
        let sexp = match self.peek_one()? {
            b'(' => {
                self.read_byte()?;
                self.parse_list_tail()?
            }
            b'[' | b'{' => {
                let xs = self.parse_collection()?;
                Sexp::Collection(xs)
            }
            b'\'' => {
                self.read_byte()?;
                self.wrap_prefix("quote")?
            }

            b'`' => {
                self.read_byte()?;
                self.wrap_prefix("quasi")?
            }

            b',' => match self.peek_two()? {
                [b',', b'@'] => {
                    self.read_byte()?;
                    self.read_byte()?;
                    self.wrap_prefix("unquote-splice")?
                }
                [b',', _] => {
                    self.read_byte()?;
                    self.wrap_prefix("unquote")?
                }
                _ => unreachable!(),
            },

            // A `)` cannot begin an expression. Inside a list this is caught by
            // `parse_list_tail`; reaching here means an unmatched close at the
            // start of a form (e.g. a top-level `)`).
            b')' => return Err(ParseError::UnexpectedCloseParen { at: self.at() }),

            // skip_trivia consumes `;;` as a comment; a `;` here is therefore
            // stray (not followed by another `;`) and never begins a form.
            b';' => return Err(ParseError::StraySemicolon { at: self.at() }),

            _ => {
                let t = self.parse_atomic()?;
                Sexp::Atom(t)
            }
        };
        Ok(sexp)
    }

    /// Wraps the following expression as `(name operand)`.
    fn wrap_prefix(&mut self, name: &str) -> Result<Sexp, ParseError> {
        let operand = self.parse_expr()?;
        let head = Sexp::Atom(Atomic::Ident(self.intern(name)));
        Ok(Sexp::Exp {
            head: Rc::new(head),
            tail: Rc::new(Sexp::Exp {
                head: Rc::new(operand),
                tail: Rc::new(Sexp::Unit),
            }),
        })
    }

    fn parse_collection(&mut self) -> Result<Collection, ParseError> {
        match self.peek_one()? {
            b'[' => self.parse_array(),
            b'{' => self.parse_map(),
            _ => unreachable!(),
        }
    }

    fn parse_atomic(&mut self) -> Result<Atomic, ParseError> {
        match self.peek_two()? {
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
            Ok(Atomic::Float(n.into()))
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
        Ok(Atomic::Ident(self.intern(s)))
    }

    /// Interns `s` into the shared identifier pool, returning the canonical
    /// `Rc<str>` so equal names share one allocation.
    fn intern(&mut self, s: &str) -> Rc<str> {
        if let Some(found) = self.idents.get(s) {
            return found.clone();
        }
        let rc: Rc<str> = s.into();
        self.idents.insert(rc.clone());
        rc
    }

    /// parses double quoted str including the opening `"` and closing `"`.
    /// Recognises `\"`, `\\`, `\n`, `\r`, `\t` escape sequences.
    /// panics if first byte isn't `"`
    fn parse_str(&mut self) -> Result<Atomic, ParseError> {
        let b = self.read_byte()?;
        assert_eq!(b, b'"');

        let mut buf: Vec<u8> = Vec::new();
        loop {
            let b = self.read_byte()?;
            match b {
                b'"' => break,
                b'\\' => {
                    let esc = self.read_byte()?;
                    match esc {
                        b'"' | b'\\' => buf.push(esc),
                        b'n' => buf.push(b'\n'),
                        b'r' => buf.push(b'\r'),
                        b't' => buf.push(b'\t'),
                        other => {
                            return Err(ParseError::InvalidEscape {
                                got: other.into(),
                                at: self.at(),
                            });
                        }
                    }
                }
                _ => buf.push(b),
            }
        }
        let at = self.at();
        let s: Rc<str> = str::from_utf8(&buf)
            .map_err(|e| ParseError::UTF8Error { source: e, at })?
            .into();
        Ok(Atomic::Str(s))
    }

    /// parses map including the opening `{` and closing `}`
    /// panics if first byte isn't `{`
    fn parse_map(&mut self) -> Result<Collection, ParseError> {
        assert_eq!(b'{', self.read_byte()?);
        let col = self.parse_map_inner(HashMap::new())?;
        let at = self.at();
        match self.read_byte()? {
            b'}' => Ok(col),
            other => Err(ParseError::ExpectedToken {
                expected: '}',
                at,
                got: other.into(),
            }),
        }
    }

    /// parse map accumulating helper
    /// first byte must NOT be `{`
    /// does not consume closing `}`
    fn parse_map_inner(
        &mut self,
        mut acc: HashMap<Rc<Sexp>, Rc<Sexp>>,
    ) -> Result<Collection, ParseError> {
        loop {
            self.skip_trivia()?;
            let k = match self.peek_one()? {
                b'}' => return Ok(Collection::Map(acc)),
                _ => self.parse_expr()?,
            };
            self.skip_trivia()?;

            match self.read_byte()? {
                b':' => {}
                other => {
                    return Err(ParseError::ExpectedToken {
                        expected: ':',
                        at: self.at(),
                        got: other.into(),
                    });
                }
            }

            let v = self.parse_expr()?;
            acc = acc.update(Rc::new(k), Rc::new(v));
        }
    }

    /// parses array including the opening `[` and closing `]`
    /// panics if first byte isn't `[`
    fn parse_array(&mut self) -> Result<Collection, ParseError> {
        assert_eq!(b'[', self.read_byte()?);
        let col = self.parse_array_inner(vec![])?;
        let at = self.at();
        match self.read_byte()? {
            b']' => Ok(col),
            other => Err(ParseError::ExpectedToken {
                expected: ']',
                at,
                got: other.into(),
            }),
        }
    }

    /// parse array accumulating helper
    /// first byte must NOT be `[`
    /// does not consume closing `]`
    fn parse_array_inner(&mut self, mut acc: Vec<Rc<Sexp>>) -> Result<Collection, ParseError> {
        loop {
            self.skip_trivia()?;
            match self.peek_one()? {
                b']' => return Ok(Collection::Array(acc.into())),
                _ => {
                    let expr = self.parse_expr()?;
                    acc.push(Rc::new(expr));
                }
            }
        }
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

    /// Peeks the next byte without consuming it, returning `None` at a clean
    /// EOF. Unlike [`peek_one`], EOF is not an error — used to check that a
    /// fully-parsed form is followed only by end-of-input.
    fn peek_eof(&mut self) -> Result<Option<u8>, ParseError> {
        let at = self.at();
        match self.reader.fill_buf() {
            Ok(avail) => Ok(avail.first().copied()),
            Err(e) => Err(ParseError::IOError { source: e, at }),
        }
    }

    fn peek_two(&mut self) -> Result<[u8; 2], ParseError> {
        let mut buf = [0u8; 2];
        self.peek_many(&mut buf)?;
        Ok(buf)
    }

    /// Peeks up to `buf.len()` bytes from the reader without consuming them.
    /// On EOF (no bytes available) returns [`ParseError::UnexpectedEof`].
    /// If fewer bytes are available than `buf.len()`, fills the prefix and
    /// leaves the rest of `buf` untouched.
    ///
    /// Known limitation: "available" means *currently buffered*. `BufRead`
    /// has no way to peek past the buffer without consuming, so when exactly
    /// one byte remains before the internal 8 KiB buffer boundary, a 2-byte
    /// lookahead (`,@`, the dotted-list marker, `;;` comment) sees only its first byte and
    /// can mis-lex. This needs a multi-byte token to straddle that exact
    /// boundary; fixing it properly means an internal peek buffer in front
    /// of the reader.
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
        ParseError::UnexpectedEof { at: self.at() }
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
            // `read_exact` reports a clean end-of-input as UnexpectedEof;
            // surface that as the dedicated variant rather than an I/O fault.
            return Err(ParseError::from_io_error(e, Some(at)));
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
                    .ok_or(ParseError::UnexpectedCloseParen { at })?;
            }
            _ => {}
        }

        Ok(buf[0])
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

    fn skip_trivia(&mut self) -> Result<(), ParseError> {
        let mut throwaway = Vec::new();
        loop {
            // `read_while` treats EOF as a clean stop, so an error here is a
            // genuine I/O fault — propagate it untouched.
            self.read_while(&mut throwaway, |b| WHITESPACE.contains(b))?;

            let starts_comment = match self.reader.fill_buf() {
                Ok(avail) => avail.len() >= 2 && avail[0] == b';' && avail[1] == b';',
                Err(e) => {
                    return Err(ParseError::IOError {
                        source: e,
                        at: self.at(),
                    });
                }
            };
            if !starts_comment {
                return Ok(());
            }

            throwaway.clear();
            self.read_until(&mut throwaway, b'\n')?;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

    // ----- helpers -----

    /// Parses `input` and asserts exactly one top-level form, returning it.
    /// Most tests cover a single form; multi-form parsing has its own tests.
    fn parse_str(input: &str) -> Result<Sexp, ParseError> {
        parse_all(input).map(|mut forms| {
            assert_eq!(forms.len(), 1, "expected exactly one top-level form");
            forms.pop().unwrap()
        })
    }

    fn parse_all(input: &str) -> Result<Vec<Sexp>, ParseError> {
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
        Sexp::Atom(Atomic::Float(f.into()))
    }
    fn ident(s: &str) -> Sexp {
        Sexp::Atom(Atomic::Ident(s.into()))
    }
    fn string(s: &str) -> Sexp {
        Sexp::Atom(Atomic::Str(s.into()))
    }

    fn map(m: HashMap<Rc<Sexp>, Rc<Sexp>>) -> Sexp {
        Sexp::Collection(Collection::Map(m))
    }

    fn array(m: &[Rc<Sexp>]) -> Sexp {
        Sexp::Collection(Collection::Array(m.into()))
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
        let (a, b) = match &parsed {
            Sexp::Exp { head, tail } => {
                let a = match &**head {
                    Sexp::Atom(Atomic::Ident(s)) => s.clone(),
                    other => panic!("expected ident, got {:?}", other),
                };
                let b = match &**tail {
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
        let (a, b) = match &parsed {
            Sexp::Exp { head, tail } => {
                let a = match &**head {
                    Sexp::Atom(Atomic::Ident(s)) => s.clone(),
                    _ => panic!(),
                };
                let b = match &**tail {
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

    // ----- collections -----
    #[test]
    fn map_of_ints() {
        let mut m = HashMap::new();
        m.insert(Rc::new(int(1)), Rc::new(int(1)));
        m.insert(Rc::new(int(2)), Rc::new(int(5)));
        assert_eq!(parse_ok("({1:1  2: 5})"), list(vec![map(m)]));
    }

    #[test]
    fn zero_element_map() {
        assert_eq!(parse_ok("({})"), list(vec![map(HashMap::new())]));
    }

    #[test]
    fn one_element_map_int() {
        let m = std::collections::HashMap::from([(Rc::new(int(1)), Rc::new(int(1)))]);
        assert_eq!(parse_ok("({1:1})"), list(vec![map(m.into())]));
    }

    #[test]
    fn one_element_map_str_with_quote() {
        let v = Rc::new(list(vec![ident("quote"), ident("foo")]));
        let m = std::collections::HashMap::from([(Rc::new(string("1")), v)]);
        assert_eq!(parse_ok(r#"({"1": 'foo})"#), list(vec![map(m.into())]));
    }

    #[test]
    fn one_element_map_str_with_quoted_key() {
        let k = Rc::new(list(vec![ident("quote"), ident("foo")]));
        let v = Rc::new(string("1"));
        let m = std::collections::HashMap::from([(k, v)]);
        assert_eq!(parse_ok(r#"({ 'foo: "1"})"#), list(vec![map(m.into())]));
    }

    #[test]
    fn array_of_ints() {
        let elems = [Rc::new(int(1)), Rc::new(int(2)), Rc::new(int(3))];
        assert_eq!(parse_ok("([1  2  3])"), list(vec![array(&elems)]));
    }

    #[test]
    fn zero_element_array() {
        assert_eq!(parse_ok("([])"), list(vec![array(&[])]));
    }

    #[test]
    fn single_element_array() {
        let elems = [Rc::new(int(42))];
        assert_eq!(parse_ok("([42])"), list(vec![array(&elems)]));
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
    fn top_level_atom_parses() {
        // A program need not be a list: a bare atom is a valid top-level form.
        assert_eq!(parse_ok("foo"), ident("foo"));
        assert_eq!(parse_ok("42"), int(42));
    }

    #[test]
    fn top_level_close_paren_is_error() {
        let err = parse_str(")").unwrap_err();
        assert!(
            matches!(err, ParseError::UnexpectedCloseParen { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn empty_input_is_error() {
        let err = parse_str("").unwrap_err();
        assert!(
            matches!(err, ParseError::UnexpectedEof { .. }),
            "got {:?}",
            err
        );
    }

    // ----- multiple top-level forms -----

    #[test]
    fn multiple_top_level_forms_collected_in_order() {
        // Two whitespace-separated lists yield two forms; the program is
        // implicitly sequenced.
        assert_eq!(
            parse_all("(1 2) (3 4)").unwrap(),
            vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])],
        );
    }

    #[test]
    fn quoted_form_followed_by_atom_is_two_forms() {
        // `'(1 2)` is a single reader-macro form `(quote (1 2))`; `foo` is the
        // second top-level form.
        let quoted = list(vec![ident("quote"), list(vec![int(1), int(2)])]);
        assert_eq!(parse_all("'(1 2) foo").unwrap(), vec![quoted, ident("foo")]);
    }

    #[test]
    fn comment_between_top_level_forms_is_skipped() {
        // Inter-form trivia (whitespace + line comments) is consumed between
        // top-level forms just like between list elements.
        assert_eq!(
            parse_all("(let x 1) ;; bind\n(+ x 2)").unwrap(),
            vec![
                list(vec![ident("let"), ident("x"), int(1)]),
                list(vec![ident("+"), ident("x"), int(2)]),
            ],
        );
    }

    #[test]
    fn trailing_whitespace_is_allowed() {
        assert_eq!(parse_ok("(1 2)  \n\t"), list(vec![int(1), int(2)]));
    }

    #[test]
    fn unterminated_list_is_eof_error() {
        let err = parse_str("(1 2").unwrap_err();
        assert!(
            matches!(err, ParseError::UnexpectedEof { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn unterminated_string_then_eof_errors() {
        let err = parse_str(r#"("abc"#).unwrap_err();
        assert!(
            matches!(err, ParseError::UnexpectedEof { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn invalid_escape_is_error() {
        let err = parse_str(r#""a\qb""#).unwrap_err();
        assert!(
            matches!(err, ParseError::InvalidEscape { got: 'q', .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn nesting_past_depth_limit_is_too_deep_error() {
        let src = "(".repeat(MAX_NESTING_DEPTH + 1);
        let err = parse_str(&src).unwrap_err();
        assert!(matches!(err, ParseError::TooDeep { .. }), "got {:?}", err);
    }

    #[test]
    fn long_flat_list_parses_without_deep_recursion() {
        // List *length* must not consume stack — neither in the parser
        // (iterative element loop) nor when the result is dropped
        // (`Sexp`'s iterative spine drop). Only nesting depth recurses.
        let mut src = String::from("(");
        for i in 0..100_000 {
            src.push_str(&i.to_string());
            src.push(' ');
        }
        src.push(')');
        let forms = parse_all(&src).expect("long flat list should parse");
        assert_eq!(forms.len(), 1);
    }

    // ----- pos tracking on error -----

    #[test]
    fn top_level_close_paren_reports_position() {
        let err = parse_str(")").unwrap_err();
        match err {
            ParseError::UnexpectedCloseParen { at } => {
                assert_eq!(at.byte, 0);
                assert_eq!(at.line, 1);
                assert_eq!(at.col, 1);
            }
            other => panic!("expected UnexpectedCloseParen, got {:?}", other),
        }
    }

    #[test]
    fn error_position_accounts_for_skipped_whitespace() {
        // skip_trivia consumes three spaces (byte = 3), then the `)` at the
        // start of the form is reported at that position.
        let err = parse_str("   )").unwrap_err();
        match err {
            ParseError::UnexpectedCloseParen { at } => {
                assert_eq!(at.byte, 3);
                assert_eq!(at.line, 1);
                assert_eq!(at.col, 4);
            }
            other => panic!("expected UnexpectedCloseParen, got {:?}", other),
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
    fn error_reports_line_and_col() {
        // Two newlines, two spaces, then ')' — ')' is at line 3, col 3.
        let err = parse_str("\n\n  )").unwrap_err();
        match err {
            ParseError::UnexpectedCloseParen { at } => {
                assert_eq!(at.byte, 4);
                assert_eq!(at.line, 3);
                assert_eq!(at.col, 3);
            }
            other => panic!("expected UnexpectedCloseParen, got {:?}", other),
        }
    }

    #[test]
    fn unexpected_close_brace_reports_line_and_col() {
        // Newline then ')' at top-level — ')' is at line 2, col 1.
        let err = parse_str("\n)").unwrap_err();
        match err {
            ParseError::UnexpectedCloseParen { at } => {
                assert_eq!(at.byte, 1);
                assert_eq!(at.line, 2);
                assert_eq!(at.col, 1);
            }
            other => panic!("expected UnexpectedCloseBrace, got {:?}", other),
        }
    }

    #[test]
    fn missing_close_brace_reports_line_and_col() {
        // Unterminated list across two lines — EOF reached at line 2, col 2.
        let err = parse_str("(1\n2").unwrap_err();
        match err {
            ParseError::UnexpectedEof { at } => {
                assert_eq!(at.line, 2);
                assert_eq!(at.col, 2);
            }
            other => panic!("expected UnexpectedEof, got {:?}", other),
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
        assert_eq!(p.parse().unwrap(), vec![list(vec![int(1), int(2)])]);
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

    // ----- line comments -----

    #[test]
    fn comment_at_top_level_before_form() {
        assert_eq!(parse_ok(";; hello\n42"), int(42));
    }

    #[test]
    fn semicolon_terminates_identifier() {
        // `foo;;bar\n` should lex as identifier `foo` then a comment.
        // The whole input is the bare atom `foo` at top level.
        assert_eq!(parse_ok("foo;;bar\n"), ident("foo"));
    }

    #[test]
    fn inline_trailing_comment() {
        assert_eq!(
            parse_ok("(1 2) ;; trailing comment\n"),
            list(vec![int(1), int(2)])
        );
    }

    #[test]
    fn comment_between_list_elements() {
        assert_eq!(parse_ok("(1 ;; middle\n 2)"), list(vec![int(1), int(2)]));
    }

    #[test]
    fn comment_ending_at_eof_without_newline() {
        // No trailing newline after the comment.
        assert_eq!(parse_ok("42 ;; bye"), int(42));
    }

    #[test]
    fn semicolon_inside_string_is_literal() {
        // The `;;` is part of the string, not a comment marker.
        assert_eq!(parse_ok(r#""a;;b""#), string("a;;b"));
    }

    #[test]
    fn comment_only_input_is_error() {
        let err = parse_str(";; just a comment\n").unwrap_err();
        assert!(
            matches!(err, ParseError::UnexpectedEof { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn position_after_comment_tracks_line() {
        // The `)` is the first non-trivia byte and sits on line 2.
        let err = parse_str(";; comment\n)").unwrap_err();
        match err {
            ParseError::UnexpectedCloseParen { at } => {
                assert_eq!(at.line, 2);
                assert_eq!(at.col, 1);
            }
            other => panic!("expected UnexpectedCloseParen, got {:?}", other),
        }
    }

    // ----- dotted lists -----

    /// Build a cons chain ending in `tail` rather than `Unit`.
    fn dotted(elems: Vec<Sexp>, tail: Sexp) -> Sexp {
        elems.into_iter().rev().fold(tail, |t, h| Sexp::Exp {
            head: Rc::new(h),
            tail: Rc::new(t),
        })
    }

    #[test]
    fn dotted_pair_two_elements() {
        // (a . b) -> Cons(a, b)
        assert_eq!(parse_ok("(a . b)"), dotted(vec![ident("a")], ident("b")));
    }

    #[test]
    fn dotted_list_collects_prefix_then_tail() {
        // (a b c . d) -> Cons(a, Cons(b, Cons(c, d)))
        assert_eq!(
            parse_ok("(a b c . d)"),
            dotted(vec![ident("a"), ident("b"), ident("c")], ident("d"))
        );
    }

    #[test]
    fn dotted_tail_can_be_arbitrary_form() {
        // (a . (b c)) -> Cons(a, Cons(b, Cons(c, Unit))) -- equivalent to (a b c)
        assert_eq!(
            parse_ok("(a . (b c))"),
            list(vec![ident("a"), ident("b"), ident("c")])
        );
    }

    #[test]
    fn dot_inside_identifier_is_not_a_marker() {
        // (foo.bar baz) -> one ident `foo.bar`, then `baz`, terminated by Unit.
        assert_eq!(
            parse_ok("(foo.bar baz)"),
            list(vec![ident("foo.bar"), ident("baz")])
        );
    }

    #[test]
    fn float_in_list_is_not_a_dotted_marker() {
        // (1.5 2) -> list of floats; the `.` inside the float never reaches the
        // inter-element peek.
        assert_eq!(parse_ok("(1.5 2)"), list(vec![float(1.5), int(2)]));
    }

    #[test]
    fn dotted_pair_with_extra_form_is_error() {
        // (a . b c) -> after the dotted tail `b`, the next byte must be `)`.
        let err = parse_str("(a . b c)").unwrap_err();
        assert!(
            matches!(err, ParseError::ExpectedToken { expected: ')', .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn stray_semicolon_is_error() {
        // A lone `;` not followed by another `;` is not a comment and not a
        // valid token. Inside a list it surfaces as StraySemicolon rather
        // than panicking (parse_ident used to assert here).
        let err = parse_str("(foo ;bar)").unwrap_err();
        assert!(
            matches!(err, ParseError::StraySemicolon { .. }),
            "got {:?}",
            err
        );
    }
}
