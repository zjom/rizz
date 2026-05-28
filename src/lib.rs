//! risp — a small Lisp interpreter.
//!
//! Source text flows through three stages:
//!
//! 1. [`parser`] reads bytes into an [`Sexp`](parser::Sexp) tree, tracking
//!    line/column [`Position`](parser::Position) for error reporting.
//! 2. [`runtime`] lowers the `Sexp` into a [`Value`] and evaluates it
//!    against an [`Env`] of bindings.
//! 3. [`prelude`] supplies the builtin functions (arithmetic, comparison,
//!    equality) that seed the default environment.
//!
//! [`parse_and_run`] wires the stages together for the common case.

use crate::runtime::{Env, Value};
use std::{io::Read, rc::Rc};

pub mod parser;
pub mod prelude;
pub mod runtime;

/// Parses one top-level form from `r` and evaluates it in a fresh environment
/// seeded with the [`prelude`]. Returns the resulting value and the final
/// environment.
pub fn parse_and_run<R: Read>(r: R) -> Result<(Rc<Value>, Env), RispError> {
    let sexp = parser::Parser::new(r).parse()?;
    let form: Value = sexp.into();
    Ok(runtime::eval(Rc::new(form), &prelude::env())?)
}

/// Like [`parse_and_run`] but evaluates against the caller-supplied `env`
/// rather than a fresh prelude, so successive calls can share bindings (e.g. a
/// REPL session that accumulates `let`/`fn` definitions).
pub fn parse_and_run_with_env<R: Read>(r: R, env: &Env) -> Result<(Rc<Value>, Env), RispError> {
    let sexp = parser::Parser::new(r).parse()?;
    let form: Value = sexp.into();
    Ok(runtime::eval(Rc::new(form), env)?)
}

/// Any failure from running a program: a [`parser::ParseError`] from reading
/// the source, or an [`runtime::RuntimeError`] from evaluating it.
#[derive(Debug, thiserror::Error)]
pub enum RispError {
    #[error(transparent)]
    ParseError(#[from] parser::ParseError),

    #[error(transparent)]
    RuntimeError(#[from] runtime::RuntimeError),
}
