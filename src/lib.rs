//! rizz — a small Lisp interpreter.
//!
//! Source text flows through three stages:
//!
//! 1. [`parser`] reads bytes into a sequence of [`Sexp`] forms,
//!    tracking line/column [`Position`](parser::Position) for error reporting.
//! 2. [`runtime`] lowers each `Sexp` into a [`Value`] and evaluates the forms
//!    against an [`Env`] of bindings, threaded across forms so a definition
//!    introduced by one form is visible to the next.
//! 3. [`prelude`] supplies the builtin functions (arithmetic, comparison,
//!    equality) that seed the default environment.
//!
//! [`parse_and_run`] wires the stages together for the common case.

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
mod repl;

use crate::parser::Sexp;
use crate::runtime::Value;
use std::{io::Read, rc::Rc};

pub mod parser;
pub use parser::{ParseError, Parser};
pub mod prelude;
pub mod runtime;
pub use runtime::{Env, RuntimeError};

/// Parses every top-level form from `r` and evaluates them in source order
/// against a fresh environment seeded with the [`prelude`]. The forms are
/// implicitly sequenced: each one's resulting env feeds the next, so later
/// forms see earlier `let`/`fn` bindings. Returns the value of the last form
/// and the final environment.
pub fn parse_and_run<R: Read>(r: R) -> Result<(Rc<Value>, Env), RizzError> {
    let forms = parser::Parser::new(r).parse()?;
    Ok(eval_forms(forms, &prelude::env())?)
}

/// Like [`parse_and_run`] but evaluates against the caller-supplied `env`
/// rather than a fresh prelude, so successive calls can share bindings (e.g. a
/// REPL session that accumulates `let`/`fn` definitions).
pub fn parse_and_run_with_env<R: Read>(r: R, env: &Env) -> Result<(Rc<Value>, Env), RizzError> {
    let forms = parser::Parser::new(r).parse()?;
    Ok(eval_forms(forms, env)?)
}

/// Evaluates `forms` in order, threading `env` between them, and returns the
/// last form's value alongside the final env. `Parser::parse` already rejects
/// empty input, so `forms` is non-empty here.
pub fn eval_forms(forms: Vec<Sexp>, env: &Env) -> Result<(Rc<Value>, Env), runtime::RuntimeError> {
    let mut env = env.clone();
    let mut last = Rc::new(Value::Unit);
    for form in forms {
        let value: Value = form.into();
        let (v, e) = runtime::eval(Rc::new(value), &env)?;
        last = v;
        env = e;
    }
    Ok((last, env))
}

/// Any failure from running a program: a [`parser::ParseError`] from reading
/// the source, or an [`runtime::RuntimeError`] from evaluating it.
#[derive(Debug, thiserror::Error)]
pub enum RizzError {
    #[error(transparent)]
    ParseError(#[from] parser::ParseError),

    #[error(transparent)]
    RuntimeError(#[from] runtime::RuntimeError),
}
