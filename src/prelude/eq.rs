//! The equality builtin.

use crate::runtime::{Env, NativeFn};
use std::rc::Rc;

/// The `=` builtin, which compares two values structurally.
pub fn env() -> Env {
    Env::of_builtins(vec![("=", eq()), ("!=", neq()), ("!", not())])
}

/// `ctx` extended with this module's builtins.
pub fn install(ctx: Env) -> Env {
    env().union(ctx)
}

/// `(= a b)`: structural equality, returning `1` if equal and `0` otherwise.
/// Functions compare by identity — distinct functions are never equal, but a
/// function equals itself (see [`Value`](crate::runtime::Value)'s `PartialEq`).
fn eq() -> NativeFn {
    NativeFn::pure("eq".into(), 2, |args| {
        Ok(Rc::new((args[0] == args[1]).into()))
    })
}

/// `(!= a b)`: structural equality, returning `1` if not equal and `0` otherwise.
/// See [eq] for semantics.
fn neq() -> NativeFn {
    NativeFn::pure("neq".into(), 2, |args| {
        Ok(Rc::new((args[0] != args[1]).into()))
    })
}

/// `(not a)`: returns `1` if a is falsy and `0` otherwise.
fn not() -> NativeFn {
    NativeFn::pure("not".into(), 1, |args| {
        Ok(Rc::new((!args[0].is_truthy()).into()))
    })
}
