//! The equality builtin.

use crate::runtime::{Env, NativeFn};
use std::rc::Rc;

/// The `=` builtin, which compares two values structurally.
pub fn env() -> Env {
    let mut entries: Vec<(&str, NativeFn)> = Vec::new();
    let mut aliases: Vec<(&str, &str)> = Vec::new();

    macro_rules! b {
        ($name:expr, $f:expr) => {
            entries.push(($name, $f()));
        };
    }
    macro_rules! alias {
        ($a:expr => $t:expr) => {
            aliases.push(($a, $t));
        };
    }

    b!("eq", eq);
    alias!("="=>"eq");
    b!("neq", neq);
    alias!("!="=>"neq");
    b!("not", not);
    alias!("!"=>"not");

    let mut env = Env::of_builtins(entries);
    for (a, t) in aliases {
        let v = env.get(&Rc::<str>::from(t)).expect("alias target").clone();
        env = env.update(a.into(), v);
    }
    env
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
    .with_doc(
        "(= a b) / (eq a b): structural equality. Returns 1 if a and b are equal, 0 otherwise. \
         Functions compare by identity — distinct functions are never equal, but a function \
         equals itself."
            .into(),
    )
}

/// `(!= a b)`: structural equality, returning `1` if not equal and `0` otherwise.
/// See [eq] for semantics.
fn neq() -> NativeFn {
    NativeFn::pure("neq".into(), 2, |args| {
        Ok(Rc::new((args[0] != args[1]).into()))
    })
    .with_doc(
        "(!= a b) / (neq a b): structural inequality. Returns 1 if a and b are not equal, \
         0 otherwise. See `eq` for semantics."
            .into(),
    )
}

/// `(not a)`: returns `1` if a is falsy and `0` otherwise.
fn not() -> NativeFn {
    NativeFn::pure("not".into(), 1, |args| {
        Ok(Rc::new((!args[0].is_truthy()).into()))
    })
    .with_doc("(! a) / (not a): returns 1 if a is falsy and 0 otherwise.".into())
}
