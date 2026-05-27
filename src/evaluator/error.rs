use crate::evaluator::Value;
use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub enum EvaluatorError {
    #[error("unknown ident {0}")]
    UnknownIdent(Rc<str>),

    #[error("cannot call {:?}", value)]
    NotCallable { value: Rc<Value> },

    #[error("arity mismatch, expected:{expected} got: {got}")]
    ArityMismatch { expected: usize, got: usize },
}
