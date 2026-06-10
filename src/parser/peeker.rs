//! A buffered reader that can reliably peek up to [`PEEK_READER_CAP`] bytes
//! ahead, regardless of where the bytes fall relative to a refill boundary.
//!
//! [`std::io::BufReader`] cannot do this: its `fill_buf` only pulls from the
//! inner source once the buffer is *fully* drained, so a multi-byte peek with
//! a single byte left buffered sees just that one byte. `PeekReader` owns its
//! buffer and, on [`peek`](PeekReader::peek), compacts the unconsumed bytes to
//! the front and reads more from the inner source until `n` bytes (or EOF) are
//! available — so two-byte lookahead (`,@`, the dotted-list `.`, `;;`) never
//! mis-lexes at a boundary.
//!
//! It implements [`Read`] and [`BufRead`], so the consuming paths
//! (`read_exact`, `read_until`, `fill_buf`/`consume`) work unchanged.

use std::io::{self, BufRead, Read};

/// Capacity of [`PeekReader`]'s internal buffer, and thus the largest peek it
/// can satisfy. Matches the default [`std::io::BufReader`] size.
pub(super) const PEEK_READER_CAP: usize = 8 * 1024;

/// A buffered reader that can reliably peek up to [`PEEK_READER_CAP`] bytes
/// ahead, regardless of where the bytes fall relative to a refill boundary.
pub struct PeekReader<R> {
    inner: R,
    buf: Box<[u8]>,
    /// Start of the unconsumed region in `buf`.
    pos: usize,
    /// End of valid (read-but-maybe-unconsumed) data in `buf`.
    cap: usize,
}

impl<R: Read> PeekReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: vec![0u8; PEEK_READER_CAP].into_boxed_slice(),
            pos: 0,
            cap: 0,
        }
    }

    /// Returns up to `n` unconsumed bytes without consuming them, reading
    /// ahead from the inner source as needed. The returned slice is shorter
    /// than `n` only at end-of-input. `n` must not exceed [`PEEK_READER_CAP`].
    pub fn peek(&mut self, n: usize) -> io::Result<&[u8]> {
        debug_assert!(n <= self.buf.len(), "peek beyond buffer capacity");
        if self.cap - self.pos < n {
            // Slide the unconsumed bytes to the front so there is room to read
            // more — this is the step a `BufReader` can't take mid-buffer.
            if self.pos > 0 {
                self.buf.copy_within(self.pos..self.cap, 0);
                self.cap -= self.pos;
                self.pos = 0;
            }
            while self.cap < n && self.cap < self.buf.len() {
                match self.inner.read(&mut self.buf[self.cap..]) {
                    Ok(0) => break, // genuine EOF
                    Ok(read) => self.cap += read,
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(&self.buf[self.pos..self.cap])
    }
}

impl<R: Read> Read for PeekReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        // With an empty buffer and a large destination, bypass our buffer to
        // avoid a redundant copy — mirrors `BufReader`'s behaviour.
        if self.pos >= self.cap && out.len() >= self.buf.len() {
            return self.inner.read(out);
        }
        let avail = self.fill_buf()?;
        let n = avail.len().min(out.len());
        out[..n].copy_from_slice(&avail[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl<R: Read> BufRead for PeekReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.pos >= self.cap {
            self.cap = self.inner.read(&mut self.buf)?;
            self.pos = 0;
        }
        Ok(&self.buf[self.pos..self.cap])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = (self.pos + amt).min(self.cap);
    }
}
