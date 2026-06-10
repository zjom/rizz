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
        "\
(array-set! REF IDX V)

Replaces the element at IDX in the array held in REF, mutating it
in place, and returns the new array.

REF — ref: must hold an array.
IDX — int: 0-based index into the array.

Errors when IDX is negative or out of bounds.

See also: (array-set ARR IDX V), (push! REF V), (pop! REF)."
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
        "\
(array-set ARR IDX V)

Returns a new array with the element at IDX replaced by V. ARR is
not mutated.

IDX — int: 0-based index into the array.

Errors when IDX is negative or out of bounds.

See also: (array-set! REF IDX V), (push ARR V)."
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
        "\
(array-from XS)

Converts XS to an array: a str splits into 1-char strs, a map
yields 2-element [K V] arrays, an array passes through, a cons
list converts element-wise, and any other value is wrapped as
[XS].

Example:
  (array-from \"abc\")     ;; => [\"a\" \"b\" \"c\"]
  (array-from {'a: 1})    ;; => [['a 1]]
  (array-from '(1 2 3))   ;; => [1 2 3]

See also: (array-of V)."
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
        "\
(array-of V)

Returns the single-element array [V] — equivalent to writing [V]
literally.

See also: (array-from XS)."
            .into(),
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
        "\
(push ARR V)

Returns a new array with V appended at the end. ARR is not
mutated.

See also: (push! REF V), (pop ARR), (concat A B)."
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
        "\
(push! REF V)

Appends V to the array held in REF, mutating it in place, and
returns the new array.

REF — ref: must hold an array.

See also: (push ARR V), (pop! REF)."
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
        "\
(pop ARR)

Returns a new array with the last element removed; an empty array
passes through unchanged. ARR is not mutated.

See also: (pop! REF), (push ARR V)."
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
        "\
(pop! REF)

Removes the last element from the array held in REF, mutating it
in place, and returns the new array.

REF — ref: must hold an array.

See also: (pop ARR), (push! REF V)."
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
        "\
(range START END)

Returns an array of the ints in the half-open interval
[START, END), empty if START >= END."
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
