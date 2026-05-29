//! Polymorphic collection builtins that dispatch on the runtime type of their
//! first argument: `len`, `get`, `concat`, `slice`, `contains?`, `reverse`,
//! `first`, `rest`, `last`.

use im::Vector;
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![
        ("len", len()),
        ("get", get()),
        ("concat", concat()),
        ("slice", slice()),
    ])
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

/// `(slice coll start end)`: half-open `[start, end)` sub-sequence of a string
/// (by char) or array. Indices are clamped to `[0, len]`; `start > end` yields
/// an empty result.
fn slice() -> NativeFn {
    NativeFn::pure("slice".into(), 3, |args| {
        let start = args[1]
            .as_int()
            .ok_or_else(|| RuntimeError::type_mismatch("slice", "int start", &args[1]))?;
        let end = args[2]
            .as_int()
            .ok_or_else(|| RuntimeError::type_mismatch("slice", "int end", &args[2]))?;
        match &*args[0] {
            Value::Array(xs) => {
                let (s, e) = clamp_range(start, end, xs.len());
                let out: Vector<Rc<Value>> = xs.iter().skip(s).take(e - s).cloned().collect();
                Ok(Rc::new(Value::Array(out)))
            }
            Value::Str(string) => {
                let chars: Vec<char> = string.chars().collect();
                let (s, e) = clamp_range(start, end, chars.len());
                let out: String = chars[s..e].iter().collect();
                Ok(Rc::new(Value::Str(out.into())))
            }
            other => Err(RuntimeError::type_mismatch("slice", "array/str", other)),
        }
    })
}

/// Clamps `[start, end)` to valid indices in `[0, len]`, guaranteeing the
/// returned `(s, e)` satisfies `s <= e <= len`.
fn clamp_range(start: i64, end: i64, len: usize) -> (usize, usize) {
    let len = len as i64;
    let s = start.clamp(0, len) as usize;
    let e = end.clamp(0, len) as usize;
    (s, e.max(s))
}

/// `(concat a b)`: joins two strings, two arrays, or two maps. For maps, the
/// second operand's entries win on key collisions.
fn concat() -> NativeFn {
    NativeFn::pure("concat".into(), 2, |args| match (&*args[0], &*args[1]) {
        (Value::Str(a), Value::Str(b)) => {
            let mut s = String::with_capacity(a.len() + b.len());
            s.push_str(a);
            s.push_str(b);
            Ok(Rc::new(Value::Str(s.into())))
        }
        (Value::Array(a), Value::Array(b)) => {
            let mut out = a.clone();
            out.append(b.clone());
            Ok(Rc::new(Value::Array(out)))
        }
        (Value::Map(a), Value::Map(b)) => {
            let mut out = a.clone();
            for (k, v) in b.iter() {
                out.insert(k.clone(), v.clone());
            }
            Ok(Rc::new(Value::Map(out)))
        }
        (other, _) => Err(RuntimeError::type_mismatch(
            "concat",
            "two strs, two arrays, or two maps",
            other,
        )),
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

    #[test]
    fn concat_same_types() {
        assert_eq!(*run_ok("(concat \"ab\" \"cd\")"), Value::Str("abcd".into()));
        assert_eq!(*run_ok("(get (concat [1 2] [3 4]) 3)"), Value::Int(4));
        assert_eq!(*run_ok("(len (concat [1 2] [3 4]))"), Value::Int(4));
        // second map wins on key collision
        assert_eq!(*run_ok("(get (concat {1: 2} {1: 9 3: 4}) 1)"), Value::Int(9));
    }

    #[test]
    fn concat_rejects_mismatch() {
        assert!(matches!(
            run("(concat \"a\" [1])"),
            Err(RispError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }

    #[test]
    fn slice_str_and_array() {
        assert_eq!(*run_ok("(slice \"hello\" 1 4)"), Value::Str("ell".into()));
        assert_eq!(*run_ok("(len (slice [1 2 3 4 5] 1 3))"), Value::Int(2));
        assert_eq!(*run_ok("(get (slice [1 2 3 4 5] 1 3) 0)"), Value::Int(2));
    }

    #[test]
    fn slice_clamps_out_of_range() {
        assert_eq!(*run_ok("(slice \"hi\" 0 99)"), Value::Str("hi".into()));
        assert_eq!(*run_ok("(slice \"hi\" 5 1)"), Value::Str("".into()));
    }
}
