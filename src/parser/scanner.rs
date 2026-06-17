//! Byte-level scanning: the layer beneath the grammar.
//!
//! [`Scanner`] owns the [`PeekReader`] and the source [`Position`], and turns
//! raw bytes into the primitives the recursive-descent grammar in the parent
//! module is built from:
//!
//! * **byte access** — `peek_one`/`peek_two`/`peek_many`/`peek_eof` (lookahead)
//!   and `read_byte`/`read_until`/`read_while` (consuming), each keeping
//!   [`Position`] in sync so every [`ParseError`] can point at the right byte;
//! * **leaf tokens** — `scan_atomic` and its helpers recognise the four
//!   [`Atomic`] kinds (string, int, float, identifier);
//! * **trivia** — `skip_trivia` drops whitespace and `;;` line comments;
//! * **interning** — identifier names share one `Rc<str>` via `intern`.
//!
//! There is deliberately no `Token` type: the grammar consumes these methods
//! directly off the byte stream, which keeps the parser fully streaming (it
//! never materialises the whole input). The split is organisational — it keeps
//! the I/O and tokenisation concerns out of the grammar — not a phase boundary.

use super::peeker::PeekReader;
use super::{Atomic, IDENT_SEPARATORS, ParseError, Position, WHITESPACE};
use std::collections::HashSet;
use std::io::{BufRead, Read};
use std::rc::Rc;

/// The byte-level reader the grammar sits on. Streams from any [`Read`] via an
/// internal [`PeekReader`], tracks source [`Position`], and interns identifier
/// names so equal names share one `Rc<str>`.
pub(super) struct Scanner<R: Read> {
    reader: PeekReader<R>,
    pos: Position,
    /// Open-paren nesting, maintained by `read_byte`, so a stray `)` is caught
    /// the moment it is consumed rather than after the surrounding form.
    list_depth: usize,
    idents: HashSet<Rc<str>>,
}

impl<R: Read> Scanner<R> {
    pub(super) fn new(r: R) -> Self {
        Self {
            reader: PeekReader::new(r),
            pos: Position::new(),
            list_depth: 0,
            idents: HashSet::new(),
        }
    }

    /// The set of identifier names interned so far.
    pub(super) fn idents(&self) -> &HashSet<Rc<str>> {
        &self.idents
    }

    /// Current position — points at the next byte to be consumed.
    pub(super) fn at(&self) -> Position {
        self.pos
    }

    // ----- leaf tokens -----

    fn scan_number(&mut self) -> Result<Atomic, ParseError> {
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

    fn scan_ident(&mut self) -> Result<Atomic, ParseError> {
        let mut buf = Vec::new();
        let n = self.read_while(&mut buf, |b| !IDENT_SEPARATORS.contains(b))?;
        assert!(n > 0);

        let at = self.at();
        let s = str::from_utf8(&buf).map_err(|e| ParseError::UTF8Error { source: e, at })?;
        Ok(Atomic::Ident(self.intern(s)))
    }

    /// parses double quoted str including the opening `"` and closing `"`.
    /// Recognises `\"`, `\\`, `\n`, `\r`, `\t` escape sequences.
    /// panics if first byte isn't `"`
    fn scan_str(&mut self) -> Result<Atomic, ParseError> {
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

    /// Scans the next leaf token, dispatching on the first byte(s): `"` opens a
    /// string, a digit (or `-` then a digit) a number, anything else an
    /// identifier.
    pub(super) fn scan_atomic(&mut self) -> Result<Atomic, ParseError> {
        match self.peek_two()? {
            [b'"', _] => self.scan_str(),
            [b'0'..=b'9', _] | [b'-', b'0'..=b'9'] => self.scan_number(),
            _ => self.scan_ident(),
        }
    }

    /// Interns `s` into the shared identifier pool, returning the canonical
    /// `Rc<str>` so equal names share one allocation.
    pub(super) fn intern(&mut self, s: &str) -> Rc<str> {
        if let Some(found) = self.idents.get(s) {
            return found.clone();
        }
        let rc: Rc<str> = s.into();
        self.idents.insert(rc.clone());
        rc
    }

    // ----- byte access -----

    pub(super) fn peek_one(&mut self) -> Result<u8, ParseError> {
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
    /// EOF. Unlike [`peek_one`](Self::peek_one), EOF is not an error — used to
    /// check that a fully-parsed form is followed only by end-of-input.
    pub(super) fn peek_eof(&mut self) -> Result<Option<u8>, ParseError> {
        let at = self.at();
        match self.reader.fill_buf() {
            Ok(avail) => Ok(avail.first().copied()),
            Err(e) => Err(ParseError::IOError { source: e, at }),
        }
    }

    pub(super) fn peek_two(&mut self) -> Result<[u8; 2], ParseError> {
        let mut buf = [0u8; 2];
        self.peek_many(&mut buf)?;
        Ok(buf)
    }

    /// Peeks up to `buf.len()` bytes from the reader without consuming them.
    /// On EOF (no bytes available) returns [`ParseError::UnexpectedEof`].
    /// If fewer bytes are available than `buf.len()`, fills the prefix and
    /// leaves the rest of `buf` untouched.
    ///
    /// [`PeekReader::peek`] reads ahead as needed, so a short fill means a
    /// genuine end-of-input — never an artefact of a refill boundary
    /// splitting a multi-byte token.
    fn peek_many(&mut self, buf: &mut [u8]) -> Result<(), ParseError> {
        let at = self.at();
        let avail = match self.reader.peek(buf.len()) {
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

    pub(super) fn read_byte(&mut self) -> Result<u8, ParseError> {
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

    // ----- trivia -----

    pub(super) fn skip_trivia(&mut self) -> Result<(), ParseError> {
        let mut throwaway = Vec::new();
        loop {
            // `read_while` treats EOF as a clean stop, so an error here is a
            // genuine I/O fault — propagate it untouched.
            self.read_while(&mut throwaway, |b| WHITESPACE.contains(b))?;

            let starts_comment = match self.reader.peek(2) {
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
