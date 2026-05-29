//! Array builtins: construction (`push`, `range`) and higher-order transforms
//! (`map`, `filter`, `reduce`). Higher-order fns are *impure* so they receive
//! the `Env` needed to invoke user closures via [`crate::runtime::apply`].

use im::Vector;
use std::rc::Rc;

use crate::runtime::{apply, Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![
        ("push", push()),
        ("range", range()),
        ("map", map()),
        ("filter", filter()),
        ("reduce", reduce()),
    ])
}

/// `(push arr v)`: a new array with `v` appended at the end.
fn push() -> NativeFn {
    NativeFn::pure("push".into(), 2, |args| match &*args[0] {
        Value::Array(xs) => {
            let mut out = xs.clone();
            out.push_back(args[1].clone());
            Ok(Rc::new(Value::Array(out)))
        }
        other => Err(RuntimeError::type_mismatch("push", "array", other)),
    })
}

/// `(range start end)`: an array of the ints `[start, end)`; empty if
/// `start >= end`.
fn range() -> NativeFn {
    NativeFn::pure("range".into(), 2, |args| {
        let start = args[0]
            .as_int()
            .ok_or_else(|| RuntimeError::type_mismatch("range", "int start", &args[0]))?;
        let end = args[1]
            .as_int()
            .ok_or_else(|| RuntimeError::type_mismatch("range", "int end", &args[1]))?;
        let out: Vector<Rc<Value>> = (start..end).map(|n| Rc::new(Value::Int(n))).collect();
        Ok(Rc::new(Value::Array(out)))
    })
}

/// `(map f arr)`: a new array of `f` applied to each element.
fn map() -> NativeFn {
    NativeFn::impure("map".into(), 2, |args, env| {
        let f = &args[0];
        match &*args[1] {
            Value::Array(xs) => {
                let mut out = Vector::new();
                for x in xs.iter() {
                    out.push_back(apply(f, std::slice::from_ref(x), env)?);
                }
                Ok((Rc::new(Value::Array(out)), env.clone()))
            }
            other => Err(RuntimeError::type_mismatch("map", "array", other)),
        }
    })
}

/// `(filter pred arr)`: a new array of the elements for which `pred` returns a
/// truthy value.
fn filter() -> NativeFn {
    NativeFn::impure("filter".into(), 2, |args, env| {
        let pred = &args[0];
        match &*args[1] {
            Value::Array(xs) => {
                let mut out = Vector::new();
                for x in xs.iter() {
                    if apply(pred, std::slice::from_ref(x), env)?.is_truthy() {
                        out.push_back(x.clone());
                    }
                }
                Ok((Rc::new(Value::Array(out)), env.clone()))
            }
            other => Err(RuntimeError::type_mismatch("filter", "array", other)),
        }
    })
}

/// `(reduce f init arr)`: left fold — `acc` starts at `init` and becomes
/// `(f acc elem)` for each element in order.
fn reduce() -> NativeFn {
    NativeFn::impure("reduce".into(), 3, |args, env| {
        let f = &args[0];
        let mut acc = args[1].clone();
        match &*args[2] {
            Value::Array(xs) => {
                for x in xs.iter() {
                    acc = apply(f, &[acc.clone(), x.clone()], env)?;
                }
                Ok((acc, env.clone()))
            }
            other => Err(RuntimeError::type_mismatch("reduce", "array", other)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RispError;

    fn run(src: &str) -> Result<Rc<Value>, RispError> {
        crate::parse_and_run(src.as_bytes()).map(|(v, _)| v)
    }
    fn run_ok(src: &str) -> Rc<Value> {
        run(src).expect("expected successful eval")
    }

    #[test]
    fn push_appends() {
        assert_eq!(*run_ok("(len (push [1 2] 3))"), Value::Int(3));
        assert_eq!(*run_ok("(get (push [1 2] 3) 2)"), Value::Int(3));
    }

    #[test]
    fn range_builds_ints() {
        assert_eq!(*run_ok("(len (range 0 5))"), Value::Int(5));
        assert_eq!(*run_ok("(get (range 2 5) 0)"), Value::Int(2));
        assert_eq!(*run_ok("(len (range 5 0))"), Value::Int(0));
    }

    #[test]
    fn map_applies_closure() {
        assert_eq!(*run_ok("(len (map (fn d (x) (* x 2)) [1 2 3]))"), Value::Int(3));
        assert_eq!(*run_ok("(get (map (fn d (x) (* x 2)) [1 2 3]) 2)"), Value::Int(6));
    }

    #[test]
    fn map_accepts_native_fn() {
        // unary use of a native fn: negate via (- 0 x) is not unary, so use to-str
        assert_eq!(*run_ok("(get (map to-str [1 2 3]) 0)"), Value::Str("1".into()));
    }

    #[test]
    fn filter_keeps_truthy() {
        // keep elements >= 2
        assert_eq!(*run_ok("(len (filter (fn p (x) (>= x 2)) [1 2 3 4]))"), Value::Int(3));
        assert_eq!(*run_ok("(get (filter (fn p (x) (>= x 2)) [1 2 3 4]) 0)"), Value::Int(2));
    }

    #[test]
    fn reduce_folds() {
        assert_eq!(*run_ok("(reduce + 0 [1 2 3 4])"), Value::Int(10));
        assert_eq!(*run_ok("(reduce (fn f (a b) (* a b)) 1 [1 2 3 4])"), Value::Int(24));
    }

    #[test]
    fn map_rejects_non_array() {
        assert!(matches!(
            run("(map to-str 5)"),
            Err(RispError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }
}
