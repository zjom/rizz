use crate::runtime::Value;
use std::rc::Rc;

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
    #[error("cannot call {:?}", value)]
    NotCallable { value: Rc<Value> },

    /// A call had the wrong number of arguments. For variadic and
    /// `nargs=0` builtins, this is only raised when the call has *fewer*
    /// than the minimum required.
    #[error("{name} failed due to arity mismatch, expected:{expected} got: {got}")]
    ArityMismatch {
        name: Rc<str>,
        expected: usize,
        got: usize,
    },

    /// An argument was the wrong type (e.g. `(car 5)` — `car` expects a
    /// cons). `expected` is a human-readable description, `got` is the
    /// variant name from [`Value::type_name`].
    #[error("{name} failed due to type mismatch, expected:{expected} got: {got}")]
    TypeMismatch {
        name: Rc<str>,
        expected: Rc<str>,
        got: Rc<str>,
    },

    /// A numeric op raised an arithmetic fault: integer overflow, division
    /// by zero, or a NaN comparison. The `reason` field carries the
    /// specific message from the failing op.
    #[error("{name} failed: {reason}")]
    ArithmeticError { name: Rc<str>, reason: Rc<str> },

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
