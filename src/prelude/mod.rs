//! The standard environment of builtin functions.
//!
//! Each submodule contributes a group of builtins; [`env()`] merges them into the
//! default environment that [`crate::parse_and_run`] evaluates against.

pub mod eq;
pub mod numbers;

use crate::evaluator::Env;

/// The default environment: every builtin from [`numbers`] and [`eq`].
pub fn env() -> Env {
    Env::new().union(numbers::env()).union(eq::env())
}

/// The default environment merged with `e`. On key collisions the prelude's
/// binding wins (see [`Env::union`]).
pub fn install(e: Env) -> Env {
    env().union(e)
}
