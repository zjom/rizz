//! Cons-cell primitives: `cons`, `car`, `cdr`. A cons list is a chain of
//! `Cons { head, tail }` values terminated by `()` (unit). To produce a
//! literal list use `(quote (a b c))`; to build one programmatically chain
//! `cons` calls or splice from another list.
//!
//! Polymorphic list operations (`len`, `get`, `first`, `fmap`, …) live in
//! [`crate::prelude::collections`] and treat `Unit` as the empty list.

use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![("cons", cons()), ("car", car()), ("cdr", cdr())])
}

/// `(cons head tail)`: a new cons cell. `tail` is typically a list (a cons
/// chain or `()`) but any value is permitted — improper pairs are allowed.
fn cons() -> NativeFn {
    NativeFn::pure("cons".into(), 2, |args| {
        Ok(Rc::new(Value::Cons {
            head: args[0].clone(),
            tail: args[1].clone(),
        }))
    })
}

/// `(car xs)`: the head of a cons cell. `(car ())` is `()`.
fn car() -> NativeFn {
    NativeFn::pure("car".into(), 1, |args| match &*args[0] {
        Value::Cons { head, .. } => Ok(head.clone()),
        Value::Unit => Ok(Rc::new(Value::Unit)),
        other => Err(RuntimeError::type_mismatch("car", "cons/()", other)),
    })
}

/// `(cdr xs)`: the tail of a cons cell. `(cdr ())` is `()`.
fn cdr() -> NativeFn {
    NativeFn::pure("cdr".into(), 1, |args| match &*args[0] {
        Value::Cons { tail, .. } => Ok(tail.clone()),
        Value::Unit => Ok(Rc::new(Value::Unit)),
        other => Err(RuntimeError::type_mismatch("cdr", "cons/()", other)),
    })
}

/// Builds a cons list from an iterator of values, terminated by `Unit`.
pub fn cons_list<I>(items: I) -> Value
where
    I: IntoIterator<Item = Rc<Value>>,
    I::IntoIter: DoubleEndedIterator,
{
    items
        .into_iter()
        .rfold(Value::Unit, |tail, head| Value::Cons {
            head,
            tail: Rc::new(tail),
        })
}

/// Whether `v` is list-shaped: a `Cons` cell or the empty list `()`.
pub fn is_list(v: &Value) -> bool {
    matches!(v, Value::Cons { .. } | Value::Unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RizzError;

    fn run(src: &str) -> Result<Rc<Value>, RizzError> {
        crate::parse_and_run(src.as_bytes()).map(|(v, _)| v)
    }
    fn run_ok(src: &str) -> Rc<Value> {
        run(src).expect("expected successful eval")
    }

    #[test]
    fn cons_builds_pair() {
        // (cons 1 ()) is a one-element list
        assert_eq!(*run_ok("(car (cons 1 ()))"), Value::Int(1));
        assert_eq!(*run_ok("(cdr (cons 1 ()))"), Value::Unit);
    }

    #[test]
    fn car_cdr_walk_a_list() {
        assert_eq!(*run_ok("(car (quote (1 2 3)))"), Value::Int(1));
        assert_eq!(*run_ok("(car (cdr (quote (1 2 3))))"), Value::Int(2));
        assert_eq!(*run_ok("(car (cdr (cdr (quote (1 2 3)))))"), Value::Int(3));
    }

    #[test]
    fn car_cdr_of_empty_is_unit() {
        assert_eq!(*run_ok("(car ())"), Value::Unit);
        assert_eq!(*run_ok("(cdr ())"), Value::Unit);
    }

    #[test]
    fn car_rejects_non_list() {
        assert!(matches!(
            run("(car 5)"),
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }
}
