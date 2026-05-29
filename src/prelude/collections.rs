//! Polymorphic collection builtins that dispatch on the runtime type of their
//! first argument: `len`, `get`, `concat`, `slice`, `contains?`, `reverse`,
//! `first`, `rest`, `last`.

use im::Vector;
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![("len", len()), ("get", get())])
}

/// `(len coll)`: element count of a str (by char), array, or map.
fn len() -> NativeFn {
    NativeFn::pure("len".into(), 1, |args| {
        let n = match &*args[0] {
            Value::Str(s) => s.chars().count() as i64,
            Value::Array(xs) => xs.len() as i64,
            Value::Map(m) => m.len() as i64,
            other => return Err(RuntimeError::type_mismatch("len", "str/array/map", other)),
        };
        Ok(Rc::new(Value::Int(n)))
    })
}

/// `(get coll k)`: map value at key `k`, array element at int index `k`, or the
/// 1-char string at int index `k`. A miss or out-of-bounds index yields `()`.
fn get() -> NativeFn {
    NativeFn::pure("get".into(), 2, |args| match &*args[0] {
        Value::Map(m) => Ok(m.get(&args[1]).cloned().unwrap_or_else(|| Rc::new(Value::Unit))),
        Value::Array(xs) => {
            let idx = args[1]
                .as_int()
                .ok_or_else(|| RuntimeError::type_mismatch("get", "int index", &args[1]))?;
            let v = usize::try_from(idx).ok().and_then(|i| xs.get(i).cloned());
            Ok(v.unwrap_or_else(|| Rc::new(Value::Unit)))
        }
        Value::Str(s) => {
            let idx = args[1]
                .as_int()
                .ok_or_else(|| RuntimeError::type_mismatch("get", "int index", &args[1]))?;
            let ch = usize::try_from(idx).ok().and_then(|i| s.chars().nth(i));
            Ok(match ch {
                Some(c) => Rc::new(Value::Str(c.to_string().into())),
                None => Rc::new(Value::Unit),
            })
        }
        other => Err(RuntimeError::type_mismatch("get", "map/array/str", other)),
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
    fn len_over_types() {
        assert_eq!(*run_ok("(len \"hello\")"), Value::Int(5));
        assert_eq!(*run_ok("(len [1 2 3])"), Value::Int(3));
        assert_eq!(*run_ok("(len {1: 2 3: 4})"), Value::Int(2));
    }

    #[test]
    fn len_rejects_non_collection() {
        assert!(matches!(
            run("(len 5)"),
            Err(RispError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }

    #[test]
    fn get_dispatches_on_type() {
        assert_eq!(*run_ok("(get {1: 2 3: 4} 3)"), Value::Int(4));
        assert_eq!(*run_ok("(get [10 20 30] 1)"), Value::Int(20));
        assert_eq!(*run_ok("(get \"abc\" 2)"), Value::Str("c".into()));
    }

    #[test]
    fn get_miss_or_out_of_bounds_is_unit() {
        assert_eq!(*run_ok("(get {1: 2} 9)"), Value::Unit);
        assert_eq!(*run_ok("(get [1 2] 9)"), Value::Unit);
        assert_eq!(*run_ok("(get \"ab\" 9)"), Value::Unit);
    }
}
