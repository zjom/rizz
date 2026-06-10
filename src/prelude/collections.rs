//! Polymorphic collection builtins that dispatch on the runtime type of
//! their collection argument: `len`, `get`, `concat`, `slice`, `contains?`,
//! `reverse`, `first`, `rest`, `last`, `find`, `all`, `any`, `zip`, and the
//! higher-order transforms `fmap`, `fmapi`, `filter`, `reduce`.
//!
//! Supported collection shapes are strings, arrays, maps, and cons lists,
//! with `()` treated as the empty list. The exact callback shape varies by
//! collection — see each builtin's doc string for the per-shape signature
//! (notably: map callbacks see `(k v)` instead of `x`, and `fmap` over a
//! map returns a fresh `[k' v']`).
//!
//! Higher-order builtins are constructed via [`NativeFn::with_env`] so they
//! can dispatch user-supplied callables through [`crate::runtime::apply`].

use crate::prelude::cons::{cons_list, is_list};
use crate::runtime::apply;
use im::{HashMap, Vector, vector};
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

/// All polymorphic collection builtins bound to their canonical names.
pub fn env() -> Env {
    Env::of_builtins(vec![
        ("all", all()),
        ("any", any()),
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
        ("fmapi", fmapi()),
        ("filter", filter()),
        ("find", find()),
        ("reduce", reduce()),
        ("zip", zip()),
    ])
}

/// `(len coll)`: element count of a str (by char), array, map, or cons list.
fn len() -> NativeFn {
    NativeFn::pure("len".into(), 1, |args| {
        let n = match &*args[0] {
            Value::Str(s) => s.chars().count() as i64,
            Value::Array(xs) => xs.len() as i64,
            Value::Map(m) => m.len() as i64,
            v if is_list(v) => Value::iter(&args[0]).count() as i64,
            other => {
                return Err(RuntimeError::type_mismatch(
                    "len",
                    "str/array/map/list",
                    other,
                ));
            }
        };
        Ok(Rc::new(Value::Int(n)))
    })
    .with_doc(
        "\
(len COLL)

Returns int: the element count of COLL — chars of a str, elements
of an array or cons list, entries of a map."
            .into(),
    )
}

/// `(slice coll start end)`: half-open `[start, end)` sub-sequence of a string
/// (by char), array, or cons list. Indices are clamped to `[0, len]`;
/// `start > end` yields an empty result.
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
            v if is_list(v) => {
                let items: Vec<Rc<Value>> = Value::iter(&args[0]).collect();
                let (s, e) = clamp_range(start, end, items.len());
                Ok(Rc::new(cons_list(items.into_iter().skip(s).take(e - s))))
            }
            other => Err(RuntimeError::type_mismatch(
                "slice",
                "array/str/list",
                other,
            )),
        }
    })
    .with_doc(
        "\
(slice COLL START END)

Returns the half-open [START, END) sub-sequence of a str (by
char), array, or cons list. Indices are clamped to [0, len];
START > END yields an empty result.

See also: (get COLL K), (first COLL), (rest COLL)."
            .into(),
    )
}

/// Clamps `[start, end)` to valid indices in `[0, len]`, guaranteeing the
/// returned `(s, e)` satisfies `s <= e <= len`.
fn clamp_range(start: i64, end: i64, len: usize) -> (usize, usize) {
    let len = len as i64;
    let s = start.clamp(0, len) as usize;
    let e = end.clamp(0, len) as usize;
    (s, e.max(s))
}

/// `(concat a b)`: joins two strings, two arrays, two maps, or two cons
/// lists. For maps, the second operand's entries win on key collisions.
fn concat() -> NativeFn {
    NativeFn::pure("concat".into(), 2, |args| {
        if is_list(&args[0]) && is_list(&args[1]) {
            let items: Vec<Rc<Value>> =
                Value::iter(&args[0]).chain(Value::iter(&args[1])).collect();
            return Ok(Rc::new(cons_list(items)));
        }
        match (&*args[0], &*args[1]) {
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
                "two strs, two arrays, two maps, or two lists",
                other,
            )),
        }
    })
    .with_doc(
        "\
(concat A B)

Joins two strs, two arrays, two maps, or two cons lists. For maps,
B's entries win on key collisions.

See also: (push ARR V), (str-join XS SEP)."
            .into(),
    )
}

/// `(get coll k)`: map value at key `k`, array/list element at int index `k`,
/// or the 1-char string at int index `k`. A miss or out-of-bounds index
/// yields `()`.
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
        v if is_list(v) => {
            let idx = args[1]
                .as_int()
                .ok_or_else(|| RuntimeError::type_mismatch("get", "int index", &args[1]))?;
            let v = usize::try_from(idx)
                .ok()
                .and_then(|i| Value::iter(&args[0]).nth(i));
            Ok(v.unwrap_or_else(|| Rc::new(Value::Unit)))
        }
        other => Err(RuntimeError::type_mismatch(
            "get",
            "map/array/str/list",
            other,
        )),
    })
    .with_doc(
        "\
(get COLL K)

Returns the map value at key K, the array or list element at int
index K, or the 1-char str at int index K. A miss or out-of-bounds
index yields ().

See also: (contains? COLL X), (slice COLL START END)."
            .into(),
    )
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
            v if is_list(v) => Value::iter(&args[0]).any(|x| x == args[1]),
            other => {
                return Err(RuntimeError::type_mismatch(
                    "contains?",
                    "str/array/map/list",
                    other,
                ));
            }
        };
        Ok(Rc::new(Value::from(result)))
    })
    .with_doc(
        "\
(contains? COLL X)

Returns 1 if X is found in COLL, else 0: a substring test for
strs, an element-equality test for arrays and cons lists, a
key-presence test for maps.

See also: (find PRED COLL), (get COLL K)."
            .into(),
    )
}

/// `(reverse coll)`: reverses a string (by char), array, or cons list.
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
        v if is_list(v) => {
            let items: Vec<Rc<Value>> = Value::iter(&args[0]).collect();
            Ok(Rc::new(cons_list(items.into_iter().rev())))
        }
        other => Err(RuntimeError::type_mismatch(
            "reverse",
            "array/str/list",
            other,
        )),
    })
    .with_doc(
        "\
(reverse COLL)

Returns a reversed copy of a str (by char), array, or cons list."
            .into(),
    )
}

/// `(first coll)`: first element of an array or cons list, or first char of a
/// string. `()` on an empty input.
fn first() -> NativeFn {
    NativeFn::pure("first".into(), 1, |args| match &*args[0] {
        Value::Array(xs) => Ok(xs.front().cloned().unwrap_or_else(|| Rc::new(Value::Unit))),
        Value::Str(s) => Ok(match s.chars().next() {
            Some(c) => Rc::new(Value::Str(c.to_string().into())),
            None => Rc::new(Value::Unit),
        }),
        Value::Cons { head, .. } => Ok(head.clone()),
        Value::Unit => Ok(Rc::new(Value::Unit)),
        other => Err(RuntimeError::type_mismatch(
            "first",
            "array/str/list",
            other,
        )),
    })
    .with_doc(
        "\
(first COLL)

Returns the first element of an array or cons list, or the first
char of a str (as a 1-char str). Returns () on an empty input.

See also: (rest COLL), (last COLL), (car XS)."
            .into(),
    )
}

/// `(last coll)`: last element of an array or cons list, or last char of a
/// string. `()` on an empty input.
fn last() -> NativeFn {
    NativeFn::pure("last".into(), 1, |args| match &*args[0] {
        Value::Array(xs) => Ok(xs.back().cloned().unwrap_or_else(|| Rc::new(Value::Unit))),
        Value::Str(s) => Ok(match s.chars().next_back() {
            Some(c) => Rc::new(Value::Str(c.to_string().into())),
            None => Rc::new(Value::Unit),
        }),
        v if is_list(v) => Ok(Value::iter(&args[0])
            .last()
            .unwrap_or_else(|| Rc::new(Value::Unit))),
        other => Err(RuntimeError::type_mismatch("last", "array/str/list", other)),
    })
    .with_doc(
        "\
(last COLL)

Returns the last element of an array or cons list, or the last
char of a str (as a 1-char str). Returns () on an empty input.

See also: (first COLL), (rest COLL)."
            .into(),
    )
}

/// `(rest coll)`: all but the first element of an array or cons list, or all
/// but the first char of a string. An empty or single-element input yields
/// an empty result.
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
        Value::Cons { tail, .. } => Ok(tail.clone()),
        Value::Unit => Ok(Rc::new(Value::Unit)),
        other => Err(RuntimeError::type_mismatch("rest", "array/str/list", other)),
    })
    .with_doc(
        "\
(rest COLL)

Returns all but the first element of an array or cons list, or all
but the first char of a str. An empty or single-element input
yields an empty result.

See also: (first COLL), (cdr XS), (slice COLL START END)."
            .into(),
    )
}

fn fmap() -> NativeFn {
    NativeFn::with_env("fmap".into(), 2, |args, env| {
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
                Ok(Rc::new(res.into()))
            }
            Value::Array(xs) => {
                let mut out = Vector::new();
                for x in xs.iter() {
                    out.push_back(apply(f, std::slice::from_ref(x), env)?);
                }
                Ok(Rc::new(Value::Array(out)))
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
                Ok(Rc::new(Value::Map(m)))
            }
            v if is_list(v) => {
                let mut out: Vec<Rc<Value>> = Vec::new();
                for x in Value::iter(&args[1]) {
                    out.push(apply(f, std::slice::from_ref(&x), env)?);
                }
                Ok(Rc::new(cons_list(out)))
            }
            other => Err(RuntimeError::type_mismatch(
                "fmap",
                "array/map/str/list",
                other,
            )),
        }
    })
    .with_doc(
        "\
(fmap F COLL)

Applies F to each element of COLL and returns a collection of the
same shape.

F    — fn: called per char for a str (as a 1-char str, must
       return a str); per element for an array or cons list; as
       (F K V) for a map, returning a 2-element [K V] array.
COLL — str | array | map | list.

Example:
  (fmap (fn d (x) (* x 2)) [1 2 3])        ;; => [2 4 6]
  (fmap (fn d (k v) [k (* v 2)]) {1: 2})   ;; => {1: 4}

See also: (fmapi F COLL), (filter PRED COLL), (reduce F INIT COLL)."
            .into(),
    )
}

fn fmapi() -> NativeFn {
    NativeFn::with_env("fmapi".into(), 2, |args, env| {
        let as_int = |i: i32| -> Rc<Value> { Rc::new((i as i64).into()) };
        let f = &args[0];
        match &*args[1] {
            Value::Str(s) => {
                let res = s.chars().zip(0..).try_fold(
                    String::with_capacity(s.len()),
                    |mut acc, (c, i)| -> Result<_, RuntimeError> {
                        let x = apply(f, &[as_int(i), Rc::new(c.to_string().into())], env)?;
                        let s = x.as_str().ok_or_else(|| {
                            RuntimeError::type_mismatch("fmapi", "lambda to return str", &x)
                        })?;

                        acc.push_str(s.as_ref());
                        Ok(acc)
                    },
                )?;
                Ok(Rc::new(res.into()))
            }
            Value::Array(xs) => {
                let mut out = Vector::new();
                for (x, i) in xs.iter().zip(0..) {
                    out.push_back(apply(f, &[as_int(i), x.clone()], env)?);
                }
                Ok(Rc::new(Value::Array(out)))
            }
            Value::Map(m) => {
                let m = m
                    .iter()
                    .zip(0..)
                    .try_fold(HashMap::new(), |acc, ((k, v), i)| {
                        let pair = apply(f, &[(as_int(i)), k.clone(), v.clone()], env)?;
                        match &*pair {
                            Value::Array(xs) => {
                                if xs.len() != 2 {
                                    return Err(RuntimeError::TypeMismatch {
                                        name: "fmapi".into(),
                                        expected: "lambda to return array of length 2".into(),
                                        got: format!("array of length {}", xs.len()).into(),
                                    });
                                }
                                Ok(acc.update(xs[0].clone(), xs[1].clone()))
                            }
                            other => Err(RuntimeError::type_mismatch(
                                "fmapi",
                                "lambda to return array of length 2",
                                other,
                            )),
                        }
                    })?;
                Ok(Rc::new(Value::Map(m)))
            }
            v if is_list(v) => {
                let mut out: Vec<Rc<Value>> = Vec::new();
                for (x, i) in Value::iter(&args[1]).zip(0..) {
                    out.push(apply(f, &[as_int(i), x.clone()], env)?);
                }
                Ok(Rc::new(cons_list(out)))
            }
            other => Err(RuntimeError::type_mismatch(
                "fmapi",
                "array/map/str/list",
                other,
            )),
        }
    })
    .with_doc(
        "\
(fmapi F COLL)

Like (fmap F COLL), but F also receives the element's index as its
first argument.

F    — fn: called as (F I CHAR) for a str (must return a str),
       (F I X) for an array or cons list, and (F I K V) for a map
       (must return a 2-element [K V] array).
COLL — str | array | map | list.

Example:
  (fmapi (fn d (i x) (+ i x)) [10 10 10])   ;; => [10 11 12]

See also: (fmap F COLL)."
            .into(),
    )
}

/// `(filter pred coll)`: keeps the parts of a collection for which `pred`
/// returns a truthy value, preserving the collection's type. For a str the
/// predicate is called per char (as a 1-char str); for an array per element;
/// for a map per entry, with the key and value passed as two args.
fn filter() -> NativeFn {
    NativeFn::with_env("filter".into(), 2, |args, env| {
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
            v if is_list(v) => {
                let mut acc: Vec<Rc<Value>> = Vec::new();
                for x in Value::iter(&args[1]) {
                    if apply(pred, std::slice::from_ref(&x), env)?.is_truthy() {
                        acc.push(x);
                    }
                }
                cons_list(acc)
            }
            other => {
                return Err(RuntimeError::type_mismatch(
                    "filter",
                    "array/map/str/list",
                    other,
                ));
            }
        };
        Ok(Rc::new(out))
    })
    .with_doc(
        "\
(filter PRED COLL)

Keeps the elements of COLL for which PRED returns truthy,
preserving the collection's type.

PRED — fn: called per char for a str (as a 1-char str); per
       element for an array or cons list; as (PRED K V) for a map.
COLL — str | array | map | list.

Example:
  (filter (fn p (x) (> x 1)) [1 2 3])   ;; => [2 3]

See also: (fmap F COLL), (find PRED COLL), (reduce F INIT COLL)."
            .into(),
    )
}

/// `(reduce f init coll)`: left fold — `acc` starts at `init`. For a str `acc`
/// becomes `(f acc char)` per char (as a 1-char str); for an array `(f acc
/// elem)` per element; for a map `(f acc k v)` per entry.
fn reduce() -> NativeFn {
    NativeFn::with_env("reduce".into(), 3, |args, env| {
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
            v if is_list(v) => {
                for x in Value::iter(&args[2]) {
                    acc = apply(f, &[acc.clone(), x], env)?;
                }
            }
            other => {
                return Err(RuntimeError::type_mismatch(
                    "reduce",
                    "array/map/str/list",
                    other,
                ));
            }
        }
        Ok(acc)
    })
    .with_doc(
        "\
(reduce F INIT COLL)

Left fold: the accumulator starts at INIT, becomes F's result at
each step, and is returned after the last element.

F    — fn: called as (F ACC CHAR) per char for a str (as a 1-char
       str), (F ACC X) per element for an array or cons list, and
       (F ACC K V) per entry for a map.
COLL — str | array | map | list.

Example:
  (reduce + 0 [1 2 3 4])   ;; => 10

See also: (fmap F COLL), (filter PRED COLL)."
            .into(),
    )
}

/// `(zip a b)`: takes two collections and produces a cons list of 2-element arrays (pairs).
/// If collection a is shorter than collection b, terminates when a terminates;
/// if b is shorter than a, terminates when b terminates.
fn zip() -> NativeFn {
    NativeFn::pure("zip".into(), 2, |args| {
        let iter_a = to_iter("zip", &args[0])?;
        let iter_b = to_iter("zip", &args[1])?;

        let pairs: Vec<Rc<Value>> = iter_a
            .zip(iter_b)
            .map(|(a, b)| Rc::new(Value::Array(vector![a, b])))
            .collect();

        Ok(Rc::new(cons_list(pairs)))
    })
    .with_doc(
        "\
(zip A B)

Pairs up the elements of two collections into a cons list of
2-element [X Y] arrays, stopping at the shorter input. Strs
iterate by char, maps yield [K V] entries, arrays and cons lists
iterate element-wise.

Example:
  (zip [1 2] \"ab\")   ;; => ([1 \"a\"] [2 \"b\"])"
            .into(),
    )
}

fn all() -> NativeFn {
    let name: Rc<str> = "all".into();
    NativeFn::with_env(name.clone(), 2, move |args, env| {
        let f = &args[0];
        let it = to_iter(&name, &args[1]).unwrap_or(Box::new(vec![args[1].clone()].into_iter()));
        for x in it {
            if !apply(f, &[x], env)?.is_truthy() {
                return Ok(Rc::new(false.into()));
            }
        }
        Ok(Rc::new(true.into()))
    })
    .with_doc(
        "\
(all PRED COLL)

Returns 1 if PRED returns truthy for every element of COLL, else
0. Short-circuits on the first falsy result; an empty COLL yields
1.

See also: (any PRED COLL), (find PRED COLL)."
            .into(),
    )
}

fn any() -> NativeFn {
    let name: Rc<str> = "any".into();
    NativeFn::with_env(name.clone(), 2, move |args, env| {
        let f = &args[0];
        let it = to_iter(&name, &args[1]).unwrap_or(Box::new(vec![args[1].clone()].into_iter()));
        for x in it {
            if apply(f, &[x], env)?.is_truthy() {
                return Ok(Rc::new(true.into()));
            }
        }
        Ok(Rc::new(false.into()))
    })
    .with_doc(
        "\
(any PRED COLL)

Returns 1 if PRED returns truthy for any element of COLL, else 0.
Short-circuits on the first truthy result; an empty COLL yields 0.

See also: (all PRED COLL), (find PRED COLL)."
            .into(),
    )
}

fn find() -> NativeFn {
    let name: Rc<str> = "find".into();
    NativeFn::with_env(name.clone(), 2, move |args, env| {
        let f = &args[0];
        if !matches!(
            &*args[1],
            Value::Str(_) | Value::Array(_) | Value::Cons { .. }
        ) {
            return Err(RuntimeError::type_mismatch(
                "find",
                "str/list/array",
                &args[1],
            ));
        }
        let it = to_iter(&name, &args[1]).unwrap_or(Box::new(vec![args[1].clone()].into_iter()));
        for (i, x) in (0..).zip(it) {
            if apply(f, &[x], env)?.is_truthy() {
                return Ok(Rc::new(i.into()));
            }
        }
        Ok(Rc::new(Value::Unit))
    })
    .with_doc(
        "\
(find PRED COLL)

Returns int: the index of the first element of COLL for which
PRED returns truthy, or () if none match.

COLL — str | array | list.

See also: (contains? COLL X), (filter PRED COLL)."
            .into(),
    )
}

fn to_iter(
    name: &str,
    val: &Rc<Value>,
) -> Result<Box<dyn Iterator<Item = Rc<Value>>>, RuntimeError> {
    match &**val {
        Value::Str(s) => {
            let chars: Vec<Rc<Value>> = s
                .chars()
                .map(|c| Rc::new(Value::Str(c.to_string().into())))
                .collect();
            Ok(Box::new(chars.into_iter()))
        }
        Value::Array(xs) => Ok(Box::new(xs.iter().cloned().collect::<Vec<_>>().into_iter())),
        Value::Map(m) => {
            let entries: Vec<Rc<Value>> = m
                .iter()
                .map(|(k, v)| Rc::new(Value::Array(vector![k.clone(), v.clone()])))
                .collect();
            Ok(Box::new(entries.into_iter()))
        }
        v if is_list(v) => {
            let items: Vec<Rc<Value>> = Value::iter(val).collect();
            Ok(Box::new(items.into_iter()))
        }
        other => Err(RuntimeError::type_mismatch(
            name,
            "array/map/str/list",
            other,
        )),
    }
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
    fn list_polymorphic_basics() {
        // len, first, last, rest, get over cons lists
        assert_eq!(*run_ok("(len (quote (1 2 3)))"), Value::Int(3));
        assert_eq!(*run_ok("(len ())"), Value::Int(0));
        assert_eq!(*run_ok("(first (quote (1 2 3)))"), Value::Int(1));
        assert_eq!(*run_ok("(last (quote (1 2 3)))"), Value::Int(3));
        assert_eq!(*run_ok("(len (rest (quote (1 2 3))))"), Value::Int(2));
        assert_eq!(*run_ok("(get (quote (10 20 30)) 1)"), Value::Int(20));
        assert_eq!(*run_ok("(get (quote (1 2)) 9)"), Value::Unit);
    }

    #[test]
    fn list_concat_slice_reverse_contains() {
        assert_eq!(
            *run_ok("(len (concat (quote (1 2)) (quote (3 4 5))))"),
            Value::Int(5)
        );
        assert_eq!(
            *run_ok("(get (concat (quote (1 2)) (quote (3 4))) 2)"),
            Value::Int(3)
        );
        assert_eq!(
            *run_ok("(len (slice (quote (1 2 3 4 5)) 1 4))"),
            Value::Int(3)
        );
        assert_eq!(*run_ok("(first (reverse (quote (1 2 3))))"), Value::Int(3));
        assert_eq!(*run_ok("(contains? (quote (1 2 3)) 2)"), Value::Int(1));
        assert_eq!(*run_ok("(contains? (quote (1 2 3)) 9)"), Value::Int(0));
    }

    #[test]
    fn list_higher_order() {
        // fmap returns a list and preserves length
        assert_eq!(
            *run_ok("(len (fmap (fn d (x) (* x 2)) (quote (1 2 3))))"),
            Value::Int(3)
        );
        assert_eq!(
            *run_ok("(first (fmap (fn d (x) (* x 2)) (quote (1 2 3))))"),
            Value::Int(2)
        );
        // filter keeps elements matching predicate
        assert_eq!(
            *run_ok("(len (filter (fn p (x) (>= x 2)) (quote (1 2 3 4))))"),
            Value::Int(3)
        );
        // reduce folds
        assert_eq!(*run_ok("(reduce + 0 (quote (1 2 3 4)))"), Value::Int(10));
    }

    #[test]
    fn higher_order_propagates_callback_arity_error() {
        // `+` is arity 2; applying it to a single element must surface the error
        assert!(matches!(
            run("(fmap + [1 2 3])"),
            Err(RizzError::RuntimeError(RuntimeError::ArityMismatch { .. }))
        ));
    }

    #[test]
    fn zip_collections() {
        // Zip two arrays of same length
        let res = run_ok("(zip [1 2] [3 4])");
        assert_eq!(res.repr(), "([1 3] [2 4])");

        // Zip two arrays of different lengths (first shorter)
        let res = run_ok("(zip [1 2] [3 4 5 6])");
        assert_eq!(res.repr(), "([1 3] [2 4])");

        // Zip two arrays of different lengths (second shorter)
        let res = run_ok("(zip [1 2 3 4] [5 6])");
        assert_eq!(res.repr(), "([1 5] [2 6])");

        // Zip with empty collection
        let res = run_ok("(zip [1 2] (range 0 0))");
        assert_eq!(res.repr(), "()");

        // Zip strings
        let res = run_ok("(zip \"ab\" \"cd\")");
        assert_eq!(res.repr(), "([\"a\" \"c\"] [\"b\" \"d\"])");

        // Zip lists
        let res = run_ok("(zip (quote (1 2)) (quote (3 4)))");
        assert_eq!(res.repr(), "([1 3] [2 4])");

        // Zip mixed types (array and list)
        let res = run_ok("(zip [1 2] (quote (3 4)))");
        assert_eq!(res.repr(), "([1 3] [2 4])");
    }
}
