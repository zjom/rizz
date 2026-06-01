//! Array construction builtins: `push` and `range`. The higher-order
//! transforms (`fmap`, `filter`, `reduce`) are polymorphic over all collections
//! and live in [`crate::prelude::collections`].

use im::Vector;
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![("push", push()), ("range", range())])
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
}
