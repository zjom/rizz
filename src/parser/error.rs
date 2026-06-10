use crate::parser::position::Position;

/// A parse failure. Every variant carries the [`Position`] where the
/// problem was detected so callers can point at the offending byte.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// A stray `)` appeared where a form was expected — typically a
    /// mismatched close at the top level.
    #[error("unexpected `)` at {at}")]
    UnexpectedCloseParen { at: Position },

    /// A lone `;` that wasn't followed by another `;`. rizz uses `;;` for
    /// line comments.
    #[error("stray `;` at {at} (use `;;` for a line comment)")]
    StraySemicolon { at: Position },

    /// Wrong delimiter — typically a missing `)`, `]`, `}`, or `:` in a
    /// map entry. `got` is the byte that turned up instead.
    #[error("expected `{expected}` {at}, got: {got}")]
    ExpectedToken {
        expected: char,
        at: Position,
        got: char,
    },

    /// The input ended before a complete form was read: empty (or
    /// comment-only) input, an unterminated list/array/map, or an
    /// unterminated string literal. REPLs treat this as "keep reading".
    #[error("unexpected end of input at {at}")]
    UnexpectedEof { at: Position },

    /// A `\` escape in a string literal that isn't one of the recognized
    /// sequences (`\"`, `\\`, `\n`, `\r`, `\t`).
    #[error("invalid escape sequence `\\{got}` at {at}")]
    InvalidEscape { got: char, at: Position },

    /// Nesting depth exceeded [`limit`](Self::TooDeep::limit) — a guard
    /// against pathological inputs overflowing the parser's stack.
    #[error("nesting depth limit ({limit}) exceeded at {at}")]
    TooDeep { at: Position, limit: usize },

    /// A byte sequence in source that isn't valid UTF-8.
    #[error("str not utf-8 at {at}: {source}")]
    UTF8Error {
        source: std::str::Utf8Error,
        at: Position,
    },

    /// A float literal that didn't parse — typically two `.`s or other
    /// malformation.
    #[error("parse float error at {at}: {source}")]
    ParseFloatError {
        source: std::num::ParseFloatError,
        at: Position,
    },

    /// An int literal that didn't parse — typically overflow past
    /// `i64::MAX`.
    #[error("parse int error at {at}: {source}")]
    ParseIntError {
        source: std::num::ParseIntError,
        at: Position,
    },

    /// An underlying I/O failure from the source reader. End-of-input is
    /// **not** reported here — that's [`UnexpectedEof`](Self::UnexpectedEof).
    #[error("io error encountered during parsing at {at}: {source}")]
    IOError {
        source: std::io::Error,
        at: Position,
    },
}

impl ParseError {
    /// Wrap an [`std::io::Error`] as a [`ParseError`] at `pos`, falling back
    /// to [`Position::default`] when no position is available.
    /// [`ErrorKind::UnexpectedEof`](std::io::ErrorKind::UnexpectedEof)
    /// becomes [`UnexpectedEof`](Self::UnexpectedEof); everything else is
    /// an [`IOError`](Self::IOError).
    pub fn from_io_error(err: std::io::Error, pos: Option<Position>) -> Self {
        let at = pos.unwrap_or_default();
        if err.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::UnexpectedEof { at }
        } else {
            Self::IOError { source: err, at }
        }
    }
}
