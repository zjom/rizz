//! The standard environment of builtin functions.
//!
//! Each submodule contributes a group of builtins; [`env()`] merges them into the
//! default environment that [`crate::parse_and_run`] evaluates against.

pub mod array;
pub mod collections;
pub mod cons;
pub mod eq;
pub mod map;
pub mod numbers;
pub mod ref_;
pub mod str;

use crate::runtime::Env;

/// The default environment: every builtin from [`numbers`], [`eq`], [`map`], [`collections`], [`mod@str`], [`mod@array`], [`cons`], [`ref_`].
pub fn env() -> Env {
    Env::new()
        .union(numbers::env())
        .union(eq::env())
        .union(map::env())
        .union(collections::env())
        .union(str::env())
        .union(array::env())
        .union(cons::env())
        .union(ref_::env())
}

/// The default environment merged with `e`. On key collisions the prelude's
/// binding wins (see [`Env::union`]).
pub fn install(e: Env) -> Env {
    env().union(e)
}
