/// A location in source text. Tracked by [`Parser`](crate::Parser) as bytes
/// are consumed and embedded in every [`ParseError`](crate::ParseError).
///
/// - `byte` is a 0-based byte offset from the start of input.
/// - `line` and `col` are 1-based — `line: 1, col: 1` is the very first
///   byte.
///
/// `Default` yields the byte/line/col-zero origin, which is what
/// `ParseError::from_io_error` falls back to when no position is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub byte: usize,
    pub line: usize,
    pub col: usize,
}

impl Position {
    /// A position at the start of input: byte 0, line 1, column 1.
    pub fn new() -> Self {
        Self {
            byte: 0,
            line: 1,
            col: 1,
        }
    }

    /// Advance the position as if `bytes` had just been read. `\n` bumps
    /// the line and resets the column to 1; every other byte advances the
    /// column. Used by the parser to keep `byte`/`line`/`col` in sync as
    /// it consumes the source.
    pub fn advance(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte += 1;
            if b == b'\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}, column {} (byte {})",
            self.line, self.col, self.byte
        )
    }
}
