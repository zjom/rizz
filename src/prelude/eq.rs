//! Equality and boolean-negation builtins: `= eq`, `!= neq`, `! not`.
//!
//! Equality is structural for data and identity for callables (see
//! [`Value`](crate::runtime::Value)'s `PartialEq`). Booleans encode as
//! ints — `1` is true, `0` is false — and follow the truthiness rule from
//! [`Value::is_truthy`](crate::runtime::Value::is_truthy).

use crate::runtime::{Env, NativeFn};
use std::rc::Rc;

/// All equality builtins together with their `=` / `!=` / `!` aliases.
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

/// Merges this module's builtins into `ctx`. On a name collision the
/// equality builtins win.
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
        "\
(= A B)
(eq A B)

Returns 1 if A and B are structurally equal, else 0. Functions
compare by identity — distinct functions are never equal, but a
function equals itself.

See also: (!= A B), (! A)."
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
        "\
(!= A B)
(neq A B)

Returns 1 if A and B are not structurally equal, else 0. The
negation of (= A B), sharing its semantics.

See also: (= A B)."
            .into(),
    )
}

/// `(not a)`: returns `1` if a is falsy and `0` otherwise.
fn not() -> NativeFn {
    NativeFn::pure("not".into(), 1, |args| {
        Ok(Rc::new((!args[0].is_truthy()).into()))
    })
    .with_doc(
        "\
(! A)
(not A)

Returns 1 if A is falsy (() or 0), else 0.

See also: (= A B)."
            .into(),
    )
}
