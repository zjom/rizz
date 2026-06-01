//! Polymorphic collection builtins that dispatch on the runtime type of their
//! collection argument: `len`, `get`, `concat`, `slice`, `contains?`,
//! `reverse`, `first`, `rest`, `last`, and the higher-order transforms `fmap`,
//! `filter`, `reduce`. The higher-order fns are *impure* so they receive the
//! `Env` needed to invoke user closures via [`crate::runtime::apply`].
//!
//! Caveat: the evaluator re-evaluates whatever value a native fn returns, so for
//! a returned array each element is evaluated a second time. Self-evaluating
//! elements (ints, floats, strings, units, and arrays/maps of those) are
//! unaffected. An `fmap`/`filter` callback that returns a non-self-evaluating
//! value — a closure or an unquoted identifier — would thus be re-evaluated by
//! the caller and misbehave; current callbacks return data, so this isn't hit.

use crate::runtime::apply;
use im::{HashMap, Vector};
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![
        ("len", len()),
        ("get", get()),
        ("concat", concat()),
        ("slice", slice()),
        ("reverse", reverse()),
        ("first", first()),
        ("rest", rest()),
        ("last", last()),
        ("contains?", contains()),
        ("fmap", fmap()),
        ("filter", filter()),
        ("reduce", reduce()),
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
        Value::Map(m) => Ok(m
            .get(&args[1])
            .cloned()
            .unwrap_or_else(|| Rc::new(Value::Unit))),
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

/// `(contains? coll x)`: substring test for strings, element-equality test for
/// arrays, key-presence test for maps. Returns `1` or `0`.
fn contains() -> NativeFn {
    NativeFn::pure("contains?".into(), 2, |args| {
        let result = match &*args[0] {
            Value::Str(s) => {
                let needle = args[1].as_str().ok_or_else(|| {
                    RuntimeError::type_mismatch("contains?", "str needle", &args[1])
                })?;
                s.contains(&*needle)
            }
            Value::Array(xs) => xs.iter().any(|x| x == &args[1]),
            Value::Map(m) => m.contains_key(&args[1]),
            other => {
                return Err(RuntimeError::type_mismatch(
                    "contains?",
                    "str/array/map",
                    other,
                ));
            }
        };
        Ok(Rc::new(Value::from(result)))
    })
}

/// `(reverse coll)`: reverses a string (by char) or array.
fn reverse() -> NativeFn {
    NativeFn::pure("reverse".into(), 1, |args| match &*args[0] {
        Value::Array(xs) => {
            let out: Vector<Rc<Value>> = xs.iter().rev().cloned().collect();
            Ok(Rc::new(Value::Array(out)))
        }
        Value::Str(s) => {
            let out: String = s.chars().rev().collect();
            Ok(Rc::new(Value::Str(out.into())))
        }
        other => Err(RuntimeError::type_mismatch("reverse", "array/str", other)),
    })
}

/// `(first coll)`: first element of an array, or first char of a string, or `()`.
fn first() -> NativeFn {
    NativeFn::pure("first".into(), 1, |args| match &*args[0] {
        Value::Array(xs) => Ok(xs.front().cloned().unwrap_or_else(|| Rc::new(Value::Unit))),
        Value::Str(s) => Ok(match s.chars().next() {
            Some(c) => Rc::new(Value::Str(c.to_string().into())),
            None => Rc::new(Value::Unit),
        }),
        other => Err(RuntimeError::type_mismatch("first", "array/str", other)),
    })
}

/// `(last coll)`: last element of an array, or last char of a string, or `()`.
fn last() -> NativeFn {
    NativeFn::pure("last".into(), 1, |args| match &*args[0] {
        Value::Array(xs) => Ok(xs.back().cloned().unwrap_or_else(|| Rc::new(Value::Unit))),
        Value::Str(s) => Ok(match s.chars().next_back() {
            Some(c) => Rc::new(Value::Str(c.to_string().into())),
            None => Rc::new(Value::Unit),
        }),
        other => Err(RuntimeError::type_mismatch("last", "array/str", other)),
    })
}

/// `(rest coll)`: all but the first element of an array, or all but the first
/// char of a string. An empty or single-element input yields an empty result.
fn rest() -> NativeFn {
    NativeFn::pure("rest".into(), 1, |args| match &*args[0] {
        Value::Array(xs) => {
            let out: Vector<Rc<Value>> = xs.iter().skip(1).cloned().collect();
            Ok(Rc::new(Value::Array(out)))
        }
        Value::Str(s) => {
            let out: String = s.chars().skip(1).collect();
            Ok(Rc::new(Value::Str(out.into())))
        }
        other => Err(RuntimeError::type_mismatch("rest", "array/str", other)),
    })
}

fn fmap() -> NativeFn {
    NativeFn::impure("fmap".into(), 2, |args, env| {
        let f = &args[0];
        match &*args[1] {
            Value::Str(s) => {
                let res = s.chars().try_fold(
                    String::with_capacity(s.len()),
                    |mut acc, c| -> Result<_, RuntimeError> {
                        let x = apply(f, &[Rc::new(c.to_string().into())], env)?;
                        let s = x.as_str().ok_or_else(|| {
                            RuntimeError::type_mismatch("fmap", "lambda to return str", &x)
                        })?;

                        acc.push_str(s.as_ref());
                        Ok(acc)
                    },
                )?;
                Ok((Rc::new(res.into()), env.clone()))
            }
            Value::Array(xs) => {
                let mut out = Vector::new();
                for x in xs.iter() {
                    out.push_back(apply(f, std::slice::from_ref(x), env)?);
                }
                Ok((Rc::new(Value::Array(out)), env.clone()))
            }
            Value::Map(m) => {
                let m = m.iter().try_fold(HashMap::new(), |acc, (k, v)| {
                    let pair = apply(f, &[k.clone(), v.clone()], env)?;
                    match &*pair {
                        Value::Array(xs) => {
                            if xs.len() != 2 {
                                return Err(RuntimeError::TypeMismatch {
                                    name: "fmap".into(),
                                    expected: "lambda to return array of length 2".into(),
                                    got: format!("array of length {}", xs.len()).into(),
                                });
                            }
                            Ok(acc.update(xs[0].clone(), xs[1].clone()))
                        }
                        other => Err(RuntimeError::type_mismatch(
                            "fmap",
                            "lambda to return array of length 2",
                            other,
                        )),
                    }
                })?;
                Ok((Rc::new(Value::Map(m)), env.clone()))
            }
            other => Err(RuntimeError::type_mismatch("fmap", "array/map/str", other)),
        }
    })
}

/// `(filter pred coll)`: keeps the parts of a collection for which `pred`
/// returns a truthy value, preserving the collection's type. For a str the
/// predicate is called per char (as a 1-char str); for an array per element;
/// for a map per entry, with the key and value passed as two args.
fn filter() -> NativeFn {
    NativeFn::impure("filter".into(), 2, |args, env| {
        let pred = &args[0];
        let out = match &*args[1] {
            Value::Str(s) => {
                let mut acc = String::new();
                for c in s.chars() {
                    let ch = Rc::new(Value::Str(c.to_string().into()));
                    if apply(pred, std::slice::from_ref(&ch), env)?.is_truthy() {
                        acc.push(c);
                    }
                }
                Value::Str(acc.into())
            }
            Value::Array(xs) => {
                let mut acc = Vector::new();
                for x in xs.iter() {
                    if apply(pred, std::slice::from_ref(x), env)?.is_truthy() {
                        acc.push_back(x.clone());
                    }
                }
                Value::Array(acc)
            }
            Value::Map(m) => {
                let mut acc = HashMap::new();
                for (k, v) in m.iter() {
                    if apply(pred, &[k.clone(), v.clone()], env)?.is_truthy() {
                        acc.insert(k.clone(), v.clone());
                    }
                }
                Value::Map(acc)
            }
            other => {
                return Err(RuntimeError::type_mismatch(
                    "filter",
                    "array/map/str",
                    other,
                ));
            }
        };
        Ok((Rc::new(out), env.clone()))
    })
}

/// `(reduce f init coll)`: left fold — `acc` starts at `init`. For a str `acc`
/// becomes `(f acc char)` per char (as a 1-char str); for an array `(f acc
/// elem)` per element; for a map `(f acc k v)` per entry.
fn reduce() -> NativeFn {
    NativeFn::impure("reduce".into(), 3, |args, env| {
        let f = &args[0];
        let mut acc = args[1].clone();
        match &*args[2] {
            Value::Str(s) => {
                for c in s.chars() {
                    let ch = Rc::new(Value::Str(c.to_string().into()));
                    acc = apply(f, &[acc.clone(), ch], env)?;
                }
            }
            Value::Array(xs) => {
                for x in xs.iter() {
                    acc = apply(f, &[acc.clone(), x.clone()], env)?;
                }
            }
            Value::Map(m) => {
                for (k, v) in m.iter() {
                    acc = apply(f, &[acc.clone(), k.clone(), v.clone()], env)?;
                }
            }
            other => {
                return Err(RuntimeError::type_mismatch(
                    "reduce",
                    "array/map/str",
                    other,
                ));
            }
        }
        Ok((acc, env.clone()))
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
    fn len_over_types() {
        assert_eq!(*run_ok("(len \"hello\")"), Value::Int(5));
        assert_eq!(*run_ok("(len [1 2 3])"), Value::Int(3));
        assert_eq!(*run_ok("(len {1: 2 3: 4})"), Value::Int(2));
    }

    #[test]
    fn len_rejects_non_collection() {
        assert!(matches!(
            run("(len 5)"),
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
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
        assert_eq!(
            *run_ok("(get (concat {1: 2} {1: 9 3: 4}) 1)"),
            Value::Int(9)
        );
    }

    #[test]
    fn concat_rejects_mismatch() {
        assert!(matches!(
            run("(concat \"a\" [1])"),
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
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

    #[test]
    fn reverse_str_and_array() {
        assert_eq!(*run_ok("(reverse \"abc\")"), Value::Str("cba".into()));
        assert_eq!(*run_ok("(get (reverse [1 2 3]) 0)"), Value::Int(3));
    }

    #[test]
    fn first_rest_last() {
        assert_eq!(*run_ok("(first [10 20 30])"), Value::Int(10));
        assert_eq!(*run_ok("(last [10 20 30])"), Value::Int(30));
        assert_eq!(*run_ok("(len (rest [10 20 30]))"), Value::Int(2));
        assert_eq!(*run_ok("(get (rest [10 20 30]) 0)"), Value::Int(20));
        assert_eq!(*run_ok("(first \"hi\")"), Value::Str("h".into()));
        assert_eq!(*run_ok("(rest \"hi\")"), Value::Str("i".into()));
    }

    #[test]
    fn first_last_of_empty_is_unit() {
        // [] is not supported by the parser; use slice to produce an empty array
        assert_eq!(*run_ok("(first (slice [1] 1 1))"), Value::Unit);
        assert_eq!(*run_ok("(last \"\")"), Value::Unit);
    }

    #[test]
    fn contains_over_types() {
        assert_eq!(*run_ok("(contains? \"hello\" \"ell\")"), Value::Int(1));
        assert_eq!(*run_ok("(contains? \"hello\" \"xyz\")"), Value::Int(0));
        assert_eq!(*run_ok("(contains? [1 2 3] 2)"), Value::Int(1));
        assert_eq!(*run_ok("(contains? [1 2 3] 9)"), Value::Int(0));
        assert_eq!(*run_ok("(contains? {1: 2 3: 4} 3)"), Value::Int(1));
        assert_eq!(*run_ok("(contains? {1: 2} 9)"), Value::Int(0));
    }

    #[test]
    fn fmap_applies_closure() {
        assert_eq!(
            *run_ok("(len (fmap (fn d (x) (* x 2)) [1 2 3]))"),
            Value::Int(3)
        );
        assert_eq!(
            *run_ok("(len (fmap (fn d (k v) [k (* v 2)]) {1:1 2:2 3:3}))"),
            Value::Int(3)
        );
        assert_eq!(
            *run_ok("(get (fmap (fn d (k v) [k (* v 2)]) {1:1 2:2 3:3}) 2)"),
            Value::Int(4)
        );
    }

    #[test]
    fn fmap_accepts_native_fn() {
        assert_eq!(
            *run_ok("(get (fmap to-str [1 2 3]) 0)"),
            Value::Str("1".into())
        );
    }

    #[test]
    fn filter_over_types() {
        // array: keep elements >= 2
        assert_eq!(
            *run_ok("(len (filter (fn p (x) (>= x 2)) [1 2 3 4]))"),
            Value::Int(3)
        );
        assert_eq!(
            *run_ok("(get (filter (fn p (x) (>= x 2)) [1 2 3 4]) 0)"),
            Value::Int(2)
        );
        // str: keep only the "l" chars
        assert_eq!(
            *run_ok("(filter (fn p (c) (= c \"l\")) \"hello\")"),
            Value::Str("ll".into())
        );
        // map: keep entries whose value > 1
        assert_eq!(
            *run_ok("(len (filter (fn p (k v) (> v 1)) {1:1 2:2 3:3}))"),
            Value::Int(2)
        );
        assert_eq!(
            *run_ok("(contains? (filter (fn p (k v) (> v 1)) {1:1 2:2 3:3}) 1)"),
            Value::Int(0)
        );
    }

    #[test]
    fn filter_can_remove_all() {
        assert_eq!(
            *run_ok("(len (filter (fn p (x) 0) [1 2 3]))"),
            Value::Int(0)
        );
        assert_eq!(
            *run_ok("(filter (fn p (c) 0) \"abc\")"),
            Value::Str("".into())
        );
    }

    #[test]
    fn reduce_over_types() {
        // array fold
        assert_eq!(*run_ok("(reduce + 0 [1 2 3 4])"), Value::Int(10));
        assert_eq!(
            *run_ok("(reduce (fn f (a b) (* a b)) 1 [1 2 3 4])"),
            Value::Int(24)
        );
        // str fold: concatenate chars onto an accumulator
        assert_eq!(
            *run_ok("(reduce concat \"\" \"abc\")"),
            Value::Str("abc".into())
        );
        // map fold: sum the values, ignoring keys
        assert_eq!(
            *run_ok("(reduce (fn f (a k v) (+ a v)) 0 {1:10 2:20 3:30})"),
            Value::Int(60)
        );
    }

    #[test]
    fn reduce_on_empty_returns_init() {
        // (range 0 0) is an empty array (empty `[]` literals are not parseable)
        assert_eq!(*run_ok("(reduce + 0 (range 0 0))"), Value::Int(0));
        assert_eq!(*run_ok("(reduce + 0 \"\")"), Value::Int(0));
    }

    #[test]
    fn higher_order_rejects_non_collection() {
        assert!(matches!(
            run("(fmap to-str 5)"),
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
        assert!(matches!(
            run("(filter (fn p (x) 1) 5)"),
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
        assert!(matches!(
            run("(reduce + 0 5)"),
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }

    #[test]
    fn higher_order_propagates_callback_arity_error() {
        // `+` is arity 2; applying it to a single element must surface the error
        assert!(matches!(
            run("(fmap + [1 2 3])"),
            Err(RizzError::RuntimeError(RuntimeError::ArityMismatch { .. }))
        ));
    }
}
