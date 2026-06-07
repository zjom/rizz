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
    /// map entry. `got` is the byte that turned up instead (`\0` for EOF
    /// inside a list).
    #[error("expected `{expected}` {at}, got: {got}")]
    ExpectedToken {
        expected: char,
        at: Position,
        got: char,
    },

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

    /// An underlying I/O failure from the source reader, **including
    /// unexpected EOF** (empty input is reported as
    /// [`ErrorKind::UnexpectedEof`](std::io::ErrorKind::UnexpectedEof)).
    #[error("io error encountered during parsing at {at}: {source}")]
    IOError {
        source: std::io::Error,
        at: Position,
    },
}

impl ParseError {
    /// Wrap an [`std::io::Error`] as a [`ParseError::IOError`] at the
    /// given position, falling back to [`Position::default`] when no
    /// position is available. Used by host code that opens a file outside
    /// the parser proper (e.g. [`crate::Runtime::eval_file`]).
    pub fn from_io_error(err: std::io::Error, pos: Option<Position>) -> Self {
        Self::IOError {
            source: err,
            at: pos.unwrap_or_default(),
        }
    }
}
