use crate::runtime::Value;
use std::rc::Rc;

/// A failure raised while evaluating a form: an unbound identifier, a call to
/// a non-callable value, a wrong argument count or type, or an arithmetic
/// fault (overflow, division by zero, NaN comparison).
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("unknown ident {0}")]
    UnknownIdent(Rc<str>),

    #[error("cannot call {:?}", value)]
    NotCallable { value: Rc<Value> },

    #[error("{name} failed due to arity mismatch, expected:{expected} got: {got}")]
    ArityMismatch {
        name: Rc<str>,
        expected: usize,
        got: usize,
    },

    #[error("{name} failed due to type mismatch, expected:{expected} got: {got}")]
    TypeMismatch {
        name: Rc<str>,
        expected: Rc<str>,
        got: Rc<str>,
    },

    #[error("{name} failed: {reason}")]
    ArithmeticError { name: Rc<str>, reason: Rc<str> },

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl RuntimeError {
    /// Builds a [`RuntimeError::TypeMismatch`] from the offending value, using
    /// its [`Value::type_name`] for the `got` field.
    pub fn type_mismatch(name: &str, expected: &str, got: &Value) -> Self {
        RuntimeError::TypeMismatch {
            name: name.into(),
            expected: expected.into(),
            got: Value::type_name(got).into(),
        }
    }
}
