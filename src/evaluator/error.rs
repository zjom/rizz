use crate::evaluator::Value;
use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub enum EvaluatorError {
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
}
