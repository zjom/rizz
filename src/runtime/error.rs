use crate::runtime::Value;
use std::{path::PathBuf, rc::Rc};

/// How many arguments a callable expected, for [`RuntimeError::ArityMismatch`].
///
/// Closures with a rest parameter and native fns check a lower bound only,
/// so a bare count would over-promise; this type keeps the error message
/// honest about which contract was violated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// Exactly `n` arguments.
    Exactly(usize),
    /// At least `n` arguments (variadic callables, min-arity native fns).
    AtLeast(usize),
    /// Between `lo` and `hi` arguments, inclusive (forms with optional args).
    Range(usize, usize),
}

impl std::fmt::Display for Arity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arity::Exactly(n) => write!(f, "{n}"),
            Arity::AtLeast(n) => write!(f, "at least {n}"),
            Arity::Range(lo, hi) => write!(f, "{lo} to {hi}"),
        }
    }
}

/// A failure raised while evaluating a form.
///
/// All variants implement [`std::error::Error`] via `thiserror`, so they
/// compose with `anyhow::Result` or any other error-aggregation strategy.
/// `IOError` and `Other` are `#[from]`-tagged so the `?` operator threads
/// the underlying error through transparently.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// An identifier was looked up in the env and not found.
    #[error("unknown ident {0}")]
    UnknownIdent(Rc<str>),

    /// A non-callable value appeared in head position of a call. The
    /// offending value is included to aid diagnosis.
    #[error("cannot call {}", value.repr())]
    NotCallable { value: Rc<Value> },

    /// A call had the wrong number of arguments. `expected` distinguishes
    /// exact-arity callables from variadic / min-arity ones (see [`Arity`]).
    #[error("{name} failed due to arity mismatch, expected: {expected}, got: {got}")]
    ArityMismatch {
        name: Rc<str>,
        expected: Arity,
        got: usize,
    },

    /// An argument was the wrong type (e.g. `(car 5)` — `car` expects a
    /// cons). `expected` is a human-readable description, `got` is the
    /// variant name from [`Value::type_name`].
    #[error("{name} failed due to type mismatch, expected: {expected}, got: {got}")]
    TypeMismatch {
        name: Rc<str>,
        expected: Rc<str>,
        got: Rc<str>,
    },

    /// An index was outside the valid range of the collection: `length` is
    /// the collection's element count, `got` the offending index.
    #[error("{name} failed due to out of bounds error, length: {length}, idx: {got}")]
    IndexOob {
        name: Rc<str>,
        length: i64,
        got: i64,
    },

    /// A numeric op raised an arithmetic fault: integer overflow, division
    /// by zero, or a NaN comparison. The `reason` field carries the
    /// specific message from the failing op.
    #[error("{name} failed: {reason}")]
    ArithmeticError { name: Rc<str>, reason: Rc<str> },

    /// A runtime parse function failed. e.g., str to int
    /// The `reason` field carries the
    /// specific message from the failing op.
    #[error("{name} failed: {reason}")]
    ParseError { name: Rc<str>, reason: Rc<str> },

    /// Evaluation recursed past the configured limit (see
    /// [`set_recursion_limit`](crate::runtime::set_recursion_limit)).
    /// Raised instead of overflowing the host stack, so embedders survive
    /// runaway recursion in user scripts.
    #[error("recursion limit ({limit}) exceeded")]
    RecursionLimit { limit: usize },

    /// A failure inside a module loaded via `(open ...)`. Preserves the
    /// module's path and the full structured error (parse or runtime) so
    /// callers can still match on the underlying variant.
    #[error("in module {}: {source}", path.display())]
    InModule {
        path: PathBuf,
        source: Box<crate::RizzError>,
    },

    /// An I/O failure from a builtin or `(open ...)` — typically a missing
    /// file or read failure.
    #[error(transparent)]
    IOError(#[from] std::io::Error),

    /// Catch-all for host-side errors injected via `anyhow` (e.g. embedded
    /// builtins that surface application-specific failures).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl RuntimeError {
    /// Build a [`TypeMismatch`](Self::TypeMismatch) from the offending
    /// value, using its [`Value::type_name`] for the `got` field. This is
    /// the convention every prelude builtin uses, so error messages stay
    /// uniform across the library.
    pub fn type_mismatch(name: &str, expected: &str, got: &Value) -> Self {
        RuntimeError::TypeMismatch {
            name: name.into(),
            expected: expected.into(),
            got: Value::type_name(got).into(),
        }
    }
}
