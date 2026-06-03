use im::{HashMap, Vector};

use crate::runtime::{RuntimeError, Value};
use std::rc::Rc;

use crate::runtime::{Env, NativeFn};

pub fn env() -> Env {
    Env::of_builtins(vec![
        ("put", put()),
        ("put!", put_bang()),
        ("keys", keys()),
        ("values", values()),
        ("del", del()),
        ("del!", del_bang()),
    ])
}

fn put() -> NativeFn {
    NativeFn::pure("put".into(), 3, |args| match &*args[0] {
        Value::Map(m) => {
            let m = m.update(args[1].clone(), args[2].clone());
            Ok(Rc::new(Value::Map(m)))
        }
        other => Err(RuntimeError::TypeMismatch {
            name: "put".into(),
            expected: Value::type_name(&Value::Map(HashMap::new())).into(),
            got: Value::type_name(other).into(),
        }),
    })
}

/// `(keys m)`: an array of the map's keys (order unspecified).
fn keys() -> NativeFn {
    NativeFn::pure("keys".into(), 1, |args| match &*args[0] {
        Value::Map(m) => {
            let out: Vector<Rc<Value>> = m.keys().cloned().collect();
            Ok(Rc::new(Value::Array(out)))
        }
        other => Err(RuntimeError::type_mismatch("keys", "map", other)),
    })
}

/// `(values m)`: an array of the map's values (order unspecified).
fn values() -> NativeFn {
    NativeFn::pure("values".into(), 1, |args| match &*args[0] {
        Value::Map(m) => {
            let out: Vector<Rc<Value>> = m.values().cloned().collect();
            Ok(Rc::new(Value::Array(out)))
        }
        other => Err(RuntimeError::type_mismatch("values", "map", other)),
    })
}

/// `(del m k)`: the map with key `k` removed (a no-op if `k` is absent).
fn del() -> NativeFn {
    NativeFn::pure("del".into(), 2, |args| match &*args[0] {
        Value::Map(m) => Ok(Rc::new(Value::Map(m.without(&args[1])))),
        other => Err(RuntimeError::type_mismatch("del", "map", other)),
    })
}

/// `(put! ref k v)`: inserts `(k → v)` into the map held in `ref` and returns
/// the new map. Errors if `ref` is not a ref, or its cell does not hold a map.
fn put_bang() -> NativeFn {
    NativeFn::pure("put!".into(), 3, |args| match &*args[0] {
        Value::Ref(cell) => {
            let new = match &*cell.borrow() {
                Value::Map(m) => Value::Map(m.update(args[1].clone(), args[2].clone())),
                other => return Err(RuntimeError::type_mismatch("put!", "ref<map>", other)),
            };
            *cell.borrow_mut() = new.clone();
            Ok(Rc::new(new))
        }
        other => Err(RuntimeError::type_mismatch("put!", "ref", other)),
    })
}

/// `(del! ref k)`: removes key `k` from the map held in `ref` (a no-op if `k`
/// is absent) and returns the new map. Errors if `ref` is not a ref, or its
/// cell does not hold a map.
fn del_bang() -> NativeFn {
    NativeFn::pure("del!".into(), 2, |args| match &*args[0] {
        Value::Ref(cell) => {
            let new = match &*cell.borrow() {
                Value::Map(m) => Value::Map(m.without(&args[1])),
                other => return Err(RuntimeError::type_mismatch("del!", "ref<map>", other)),
            };
            *cell.borrow_mut() = new.clone();
            Ok(Rc::new(new))
        }
        other => Err(RuntimeError::type_mismatch("del!", "ref", other)),
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
    fn keys_and_values() {
        assert_eq!(*run_ok("(len (keys {1: 2 3: 4}))"), Value::Int(2));
        assert_eq!(*run_ok("(len (values {1: 2 3: 4}))"), Value::Int(2));
        assert_eq!(*run_ok("(contains? (keys {1: 2 3: 4}) 1)"), Value::Int(1));
        assert_eq!(*run_ok("(contains? (values {1: 2 3: 4}) 4)"), Value::Int(1));
    }

    #[test]
    fn del_removes_key() {
        assert_eq!(*run_ok("(len (del {1: 2 3: 4} 1))"), Value::Int(1));
        assert_eq!(*run_ok("(get (del {1: 2 3: 4} 1) 1)"), Value::Unit);
        // deleting an absent key is a no-op
        assert_eq!(*run_ok("(len (del {1: 2} 9))"), Value::Int(1));
    }

    #[test]
    fn keys_rejects_non_map() {
        assert!(matches!(
            run("(keys [1 2])"),
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }
}
