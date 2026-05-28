//! The equality builtin.

use crate::runtime::RuntimeError;
use crate::runtime::{BuiltinFn, Env};
use std::rc::Rc;

/// The `=` builtin, which compares two values structurally.
pub fn env() -> Env {
    Env::of_builtins(vec![("=", eq())])
}

/// `ctx` extended with this module's builtins.
pub fn install(ctx: Env) -> Env {
    env().union(ctx)
}

/// `(= a b)`: structural equality, returning `1` if equal and `0` otherwise.
/// Functions never compare equal (see [`Value`](crate::runtime::Value)'s
/// `PartialEq`).
fn eq() -> BuiltinFn {
    let name = "eq";
    Rc::new(move |args, env| {
        if args.len() != 2 {
            return Err(RuntimeError::ArityMismatch {
                name: name.into(),
                expected: 2,
                got: args.len(),
            });
        }
        let v = Rc::new((args[0] == args[1]).into());
        Ok((v, env.clone()))
    })
}
