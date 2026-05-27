use crate::evaluator::{Env, Value};
use std::{io::Read, rc::Rc};

pub mod evaluator;
pub mod parser;
pub mod prelude;

pub fn parse_and_run<R: Read>(r: R) -> Result<Rc<Value>, RispError> {
    let sexp = parser::Parser::new(r).parse()?;
    let form: Value = sexp.into();
    Ok(evaluator::eval(Rc::new(form), &prelude::env())?)
}

pub fn parse_and_run_with_env<R: Read>(r: R, env: &Env) -> Result<Rc<Value>, RispError> {
    let sexp = parser::Parser::new(r).parse()?;
    let form: Value = sexp.into();
    Ok(evaluator::eval(Rc::new(form), env)?)
}

#[derive(Debug, thiserror::Error)]
pub enum RispError {
    #[error(transparent)]
    ParseError(#[from] parser::ParseError),

    #[error(transparent)]
    RuntimeError(#[from] evaluator::EvaluatorError),
}
