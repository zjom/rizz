//! Polymorphic collection builtins that dispatch on the runtime type of their
//! first argument: `len`, `get`, `concat`, `slice`, `contains?`, `reverse`,
//! `first`, `rest`, `last`.

use im::Vector;
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![("len", len())])
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
}
