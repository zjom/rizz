use crate::parser::position::Position;

/// A parse failure, carrying the [`Position`] where it was detected. Variants
/// cover unbalanced parentheses, malformed numbers/strings, non-UTF-8 input,
/// and underlying I/O errors (including unexpected EOF).
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("expected open brace at {at}")]
    MissingOpenBrace { at: Position },
    #[error("expected close brace at {at}")]
    MissingCloseBrace { at: Position },
    #[error("unexpected close brace at {at}")]
    UnexpectedCloseBrace { at: Position },

    #[error("str not utf-8 at {at}: {source}")]
    UTF8Error {
        source: std::str::Utf8Error,
        at: Position,
    },
    #[error("string not utf-8 at {at}: {source}")]
    FromUTF8Error {
        source: std::string::FromUtf8Error,
        at: Position,
    },

    #[error("parse float error at {at}: {source}")]
    ParseFloatError {
        source: std::num::ParseFloatError,
        at: Position,
    },

    #[error("parse int error at {at}: {source}")]
    ParseIntError {
        source: std::num::ParseIntError,
        at: Position,
    },

    #[error("io error encountered during parsing at {at}: {source}")]
    IOError {
        source: std::io::Error,
        at: Position,
    },
}
