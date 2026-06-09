//! Array builtins: construction (`push`, `pop`, `range`, `array-of`,
//! `array-from`, `array-set`) and the in-place variants `push!` / `pop!` /
//! `array-set!`.
//!
//! Arrays are persistent ([`im::Vector`]); the unsuffixed ops return a new
//! array sharing structure with the input, the `!` variants mutate an
//! array held in a [`Value::Ref`].
//!
//! The polymorphic transforms (`fmap`, `filter`, `reduce`, `len`, `get`, …)
//! work on arrays too — they live in [`crate::prelude::collections`].

use im::{Vector, vector};
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

/// All array builtins bound to their canonical names.
pub fn env() -> Env {
    Env::of_builtins(vec![
        ("push", push()),
        ("push!", push_bang()),
        ("pop", pop()),
        ("pop!", pop_bang()),
        ("range", range()),
        ("array-of", of()),
        ("array-from", from()),
        ("array-set", set()),
        ("array-set!", set_bang()),
    ])
}

fn set_bang() -> NativeFn {
    let name: Rc<str> = "array-set!".into();
    NativeFn::pure(name.clone(), 3, move |args| {
        let cell = match &*args[0] {
            Value::Ref(cell) => cell,
            other => return Err(RuntimeError::type_mismatch(&name, "ref", other)),
        };
        let idx = args[1]
            .as_int()
            .ok_or_else(|| RuntimeError::type_mismatch(&name, "pos int", &args[1]))?;
        if idx < 0 {
            return Err(RuntimeError::type_mismatch(&name, "pos int", &args[1]));
        }
        let new = match &*cell.borrow() {
            Value::Array(xs) => {
                if idx >= xs.len() as i64 {
                    return Err(RuntimeError::IndexOob {
                        name: name.clone(),
                        length: xs.len() as i64,
                        got: idx,
                    });
                }
                Value::Array(xs.update(idx as usize, args[2].clone()))
            }
            other => return Err(RuntimeError::type_mismatch(&name, "ref<array>", other)),
        };
        *cell.borrow_mut() = new.clone();
        Ok(Rc::new(new))
    })
    .with_doc(
        "(array-set! ref idx v): replaces the element at idx in the array held in ref \
         (mutating it) and returns the new array. Errors if ref is not a ref or does \
         not hold an array, if idx is not a non-negative int, or if idx is out of bounds."
            .into(),
    )
}
fn set() -> NativeFn {
    let name: Rc<str> = "array-set".into();
    NativeFn::pure(name.clone(), 3, move |args| {
        let arr = args[0]
            .as_array()
            .ok_or_else(|| RuntimeError::type_mismatch(&name, "array", &args[0]))?;
        let idx = args[1]
            .as_int()
            .ok_or_else(|| RuntimeError::type_mismatch(&name, "pos int", &args[1]))?;
        if idx < 0 {
            return Err(RuntimeError::type_mismatch(&name, "pos int", &args[1]));
        }
        if idx >= arr.len() as i64 {
            return Err(RuntimeError::IndexOob {
                name: name.clone(),
                length: arr.len() as i64,
                got: idx,
            });
        }

        Ok(Rc::new(Value::Array(
            arr.update(idx as usize, args[2].clone()),
        )))
    })
    .with_doc(
        "(array-set arr idx v): a new array with the element at idx replaced by v. \
         arr is not mutated. Errors if arr is not an array, if idx is not a \
         non-negative int, or if idx is out of bounds."
            .into(),
    )
}

/// `(array-from xs)`: constructs an array from `xs`
/// output array shape depends on the type of `xs`
///
/// string => array of len 1 strings
/// map => array of [key, value]
/// array => self
/// list => array following [Value::iter] semantics
/// other => [other]
///
/// `(array-from "abc")` => `["a" "b" "c"]`
/// `(array-from {'a:1 'b:2 'c:3})` => [['a 1] ['b 2] ['c 3]]
/// `(array-from [1 2 3])` => [1 2 3]
/// `(array-from '(1 2 3))` => [1 2 3]
/// `(array-from 123)` => 123
fn from() -> NativeFn {
    NativeFn::pure("array-from".into(), 1, |args| {
        Ok(Rc::new(Value::Array(match args[0].as_ref() {
            Value::Str(s) => s
                .chars()
                .map(|b| Rc::new(Value::Str(b.to_string().into())))
                .collect::<Vector<_>>(),
            Value::Map(m) => m
                .into_iter()
                .map(|(k, v)| Rc::new(Value::Array(vector![k.clone(), v.clone()])))
                .collect(),
            Value::Array(xs) => xs.clone(),
            _ => Value::iter(&args[0]).collect(),
        })))
    })
    .with_doc(
        "(array-from xs): converts xs to an array. Strings split into 1-char strings, \
         maps yield [key value] pairs, arrays pass through, lists are flattened, \
         and any other value is wrapped in [x]."
            .into(),
    )
}

/// `(array-of v)`: constructs an array with a single value.
/// this is equivalent to `[v]`
fn of() -> NativeFn {
    NativeFn::pure("array-of".into(), 1, |args| {
        Ok(Rc::new(Value::Array(Vector::unit(args[0].clone()))))
    })
    .with_doc(
        "(array-of v): a single-element array [v]. Equivalent to writing [v] literally.".into(),
    )
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
    .with_doc(
        "(push arr v): a new array with v appended at the end. \
         arr is not mutated. Errors if arr is not an array."
            .into(),
    )
}

/// `(push! ref v)`: appends `v` to the array held in `ref` and returns the new
/// array. Errors if `ref` is not a ref, or its cell does not hold an array.
fn push_bang() -> NativeFn {
    NativeFn::pure("push!".into(), 2, |args| match &*args[0] {
        Value::Ref(cell) => {
            let new = match &*cell.borrow() {
                Value::Array(xs) => {
                    let mut out = xs.clone();
                    out.push_back(args[1].clone());
                    Value::Array(out)
                }
                other => return Err(RuntimeError::type_mismatch("push!", "ref<array>", other)),
            };
            *cell.borrow_mut() = new.clone();
            Ok(Rc::new(new))
        }
        other => Err(RuntimeError::type_mismatch("push!", "ref", other)),
    })
    .with_doc(
        "(push! ref v): appends v to the array held in ref (mutating it) and returns \
         the new array. Errors if ref is not a ref or does not hold an array."
            .into(),
    )
}

/// `(pop arr)`: a new array with the last element removed. Returns an empty
/// array unchanged.
fn pop() -> NativeFn {
    NativeFn::pure("pop".into(), 1, |args| match &*args[0] {
        Value::Array(xs) => {
            let mut out = xs.clone();
            out.pop_back();
            Ok(Rc::new(Value::Array(out)))
        }
        other => Err(RuntimeError::type_mismatch("pop", "array", other)),
    })
    .with_doc(
        "(pop arr): a new array with the last element removed. \
         An empty array passes through unchanged. arr is not mutated."
            .into(),
    )
}

/// `(pop! ref)`: removes the last element from the array held in `ref` and
/// returns the new array. Errors if `ref` is not a ref, or its cell does not
/// hold an array.
fn pop_bang() -> NativeFn {
    NativeFn::pure("pop!".into(), 1, |args| match &*args[0] {
        Value::Ref(cell) => {
            let new = match &*cell.borrow() {
                Value::Array(xs) => {
                    let mut out = xs.clone();
                    out.pop_back();
                    Value::Array(out)
                }
                other => return Err(RuntimeError::type_mismatch("pop!", "ref<array>", other)),
            };
            *cell.borrow_mut() = new.clone();
            Ok(Rc::new(new))
        }
        other => Err(RuntimeError::type_mismatch("pop!", "ref", other)),
    })
    .with_doc(
        "(pop! ref): removes the last element from the array held in ref (mutating it) \
         and returns the new array. Errors if ref is not a ref or does not hold an array."
            .into(),
    )
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
    .with_doc(
        "(range start end): an array of the ints in the half-open interval [start, end). \
         Empty if start >= end. Both args must be ints."
            .into(),
    )
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
