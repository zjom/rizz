//! Arithmetic, comparison, and numeric-conversion builtins.
//!
//! Every operator here is binary and works on two ints or two floats (never
//! a mix — there is no implicit numeric coercion; `int-of` and `float-of`
//! are the explicit conversions). The operators share the generic `binop`
//! machinery, which dispatches on the argument type. Comparisons return
//! `1` for true and `0` for false.
//!
//! # Fault policy
//!
//! - **Int ops are checked**: overflow and division by zero raise
//!   [`RuntimeError::ArithmeticError`].
//! - **Float ops follow IEEE-754**: division by zero yields ±`inf`, and
//!   `0.0 / 0.0` yields NaN, silently. NaN *propagates* through arithmetic
//!   but is rejected wherever an ordering is required — `cmp`, `min`,
//!   `max`, and `clamp` raise an [`ArithmeticError`] when they encounter
//!   one ([`RuntimeError::ArithmeticError`]).
//!
//! Refs are transparently dereferenced for numeric ops, so e.g.
//! `(+ (ref 5) 1)` evaluates to `6` without an explicit `deref`. See
//! [`Numeric`] for how new numeric types could be
//! plugged in.

use crate::runtime::Numeric;
use std::rc::Rc;
use std::str::FromStr;

use crate::runtime::{Arity, Env, NativeFn, RuntimeError, Value};

/// The arithmetic/comparison builtins: `+ - * /`, `mod`, `cmp`,
/// `> >= < <=`, `min`/`max`/`clamp`, and the conversions
/// `int-of`/`float-of`.
pub fn env() -> Env {
    let mut entries: Vec<(&str, NativeFn)> = Vec::new();
    let mut aliases: Vec<(&str, &str)> = Vec::new();

    macro_rules! b {
        ($name:expr, $f:expr) => {
            entries.push(($name, $f()));
        };
    }
    macro_rules! alias {
        ($a:expr => $t:expr) => {
            aliases.push(($a, $t));
        };
    }
    b!("sum", add);
    alias!("+"=>"sum");
    b!("sub", sub);
    alias!("-"=>"sub");
    b!("mul", mul);
    alias!("*"=>"mul");
    b!("div", div);
    alias!("/"=>"div");
    b!("mod", _mod);
    b!("int-of", int_of);
    b!("float-of", float_of);
    b!("cmp", cmp);
    b!("gt", gt);
    alias!(">" => "gt");
    b!("gte", gte);
    alias!(">=" => "gte");
    b!("lt", lt);
    alias!("<" => "lt");
    b!("lte", lte);
    alias!("<=" => "lte");
    b!("min", min);
    b!("max", max);
    b!("clamp", clamp);

    let mut env = Env::of_builtins(entries);
    for (a, t) in aliases {
        let v = env.get(&Rc::<str>::from(t)).expect("alias target").clone();
        env = env.update(a.into(), v);
    }
    env
}

/// `ctx` extended with this module's builtins. On key collision `ctx`
/// wins, matching [`crate::prelude::install`].
pub fn install(ctx: Env) -> Env {
    ctx.union(env())
}

fn add() -> NativeFn {
    binop(
        "add",
        |a, b| a.checked_add(b).ok_or("integer overflow"),
        |a, b| Ok(a + b),
    )
    .with_doc(
        "\
(+ A B)
(sum A B)

Adds two ints or two floats (never mixed — there is no implicit
numeric coercion).

Errors when the int result overflows.

See also: (- A B), (* A B), (/ A B)."
            .into(),
    )
}
fn sub() -> NativeFn {
    binop(
        "sub",
        |a, b| a.checked_sub(b).ok_or("integer overflow"),
        |a, b| Ok(a - b),
    )
    .with_doc(
        "\
(- A B)
(sub A B)

Subtracts B from A — two ints or two floats (never mixed).

Errors when the int result overflows.

See also: (+ A B), (* A B), (/ A B)."
            .into(),
    )
}

fn mul() -> NativeFn {
    binop(
        "mul",
        |a, b| a.checked_mul(b).ok_or("integer overflow"),
        |a, b| Ok(a * b),
    )
    .with_doc(
        "\
(* A B)
(mul A B)

Multiplies two ints or two floats (never mixed).

Errors when the int result overflows.

See also: (+ A B), (- A B), (/ A B)."
            .into(),
    )
}

fn div() -> NativeFn {
    binop(
        "div",
        |a, b| a.checked_div(b).ok_or("division by zero"),
        |a, b| Ok(a / b),
    )
    .with_doc(
        "\
(/ A B)
(div A B)

Divides A by B — two ints (truncating) or two floats (never mixed).
Float division follows IEEE-754: dividing by 0.0 yields ±inf, and
0.0 / 0.0 yields NaN, silently.

Errors when dividing an int by zero.

See also: (+ A B), (- A B), (* A B)."
            .into(),
    )
}

fn _mod() -> NativeFn {
    binop(
        "mod",
        |a, b| a.checked_rem_euclid(b).ok_or("division by zero"),
        |a, b| Ok(a.rem_euclid(b)),
    )
    .with_doc(
        "\
(mod A B)

Returns the least nonnegative remainder of A divided by B — two
ints or two floats (never mixed).

Errors when dividing an int by zero.

See also: (+ A B), (- A B), (* A B), (/ A B)."
            .into(),
    )
}

fn cmp() -> NativeFn {
    binop(
        "cmp",
        |a, b| {
            Ok(match a.cmp(&b) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Greater => 1,
                std::cmp::Ordering::Equal => 0,
            })
        },
        |a, b| {
            a.partial_cmp(&b)
                .map(|o| match o {
                    std::cmp::Ordering::Less => -1.,
                    std::cmp::Ordering::Greater => 1.,
                    std::cmp::Ordering::Equal => 0.,
                })
                .ok_or("comparison with NaN")
        },
    )
    .with_doc(
        "\
(cmp A B)

Three-way numeric comparison of two ints or two floats (never
mixed). Returns -1 if A < B, 0 if A = B, and 1 if A > B.

Errors when a NaN is involved.

See also: (< A B), (<= A B), (> A B), (>= A B)."
            .into(),
    )
}

fn gt() -> NativeFn {
    binop("gt", |a, b| Ok(a > b), |a, b| Ok(a > b)).with_doc(
        "\
(> A B)
(gt A B)

Returns 1 if A > B, else 0. Compares two ints or two floats (never
mixed).

See also: (>= A B), (< A B), (cmp A B)."
            .into(),
    )
}

fn gte() -> NativeFn {
    binop("gte", |a, b| Ok(a >= b), |a, b| Ok(a >= b)).with_doc(
        "\
(>= A B)
(gte A B)

Returns 1 if A >= B, else 0. Compares two ints or two floats (never
mixed).

See also: (> A B), (<= A B), (cmp A B)."
            .into(),
    )
}

fn lt() -> NativeFn {
    binop("lt", |a, b| Ok(a < b), |a, b| Ok(a < b)).with_doc(
        "\
(< A B)
(lt A B)

Returns 1 if A < B, else 0. Compares two ints or two floats (never
mixed).

See also: (<= A B), (> A B), (cmp A B)."
            .into(),
    )
}

fn lte() -> NativeFn {
    binop("lte", |a, b| Ok(a <= b), |a, b| Ok(a <= b)).with_doc(
        "\
(<= A B)
(lte A B)

Returns 1 if A <= B, else 0. Compares two ints or two floats (never
mixed).

See also: (< A B), (>= A B), (cmp A B)."
            .into(),
    )
}

/// Shared implementation for `min` / `max`: a simple scan that requires a
/// homogeneous numeric type and rejects NaN (consistent with `cmp`).
fn extremum(name: &'static str, pick_max: bool) -> NativeFn {
    NativeFn::pure(name.into(), 0, move |args| {
        if args.is_empty() {
            return Err(RuntimeError::ArityMismatch {
                name: name.into(),
                expected: Arity::AtLeast(1),
                got: 0,
            });
        }

        // Either one collection argument or variadic scalars.
        let items: Vec<Rc<Value>> = match args[0].as_ref() {
            Value::Array(_) | Value::Cons { .. } => {
                if args.len() != 1 {
                    return Err(RuntimeError::ArityMismatch {
                        name: name.into(),
                        expected: Arity::Exactly(1),
                        got: args.len(),
                    });
                }
                match args[0].as_ref() {
                    Value::Array(xs) => xs.iter().cloned().collect(),
                    _ => Value::iter(&args[0]).collect(),
                }
            }
            _ => args.to_vec(),
        };

        let type_err = || RuntimeError::TypeMismatch {
            name: name.into(),
            expected: "number* | [number*] | '(number*) (homogeneous)".into(),
            got: items
                .iter()
                .map(|v| Value::type_name(v))
                .collect::<Vec<_>>()
                .join("*")
                .into(),
        };

        match items.first().map(|v| v.as_ref()) {
            None => Err(RuntimeError::TypeMismatch {
                name: name.into(),
                expected: "non-empty collection".into(),
                got: "empty collection".into(),
            }),
            Some(Value::Int(_)) => {
                let mut best: Option<i64> = None;
                for item in &items {
                    let n = item.as_int().ok_or_else(type_err)?;
                    best = Some(match best {
                        None => n,
                        Some(b) if pick_max => b.max(n),
                        Some(b) => b.min(n),
                    });
                }
                Ok(Rc::new(Value::Int(best.expect("items is non-empty"))))
            }
            Some(Value::Float(_)) => {
                let mut best: Option<f64> = None;
                for item in &items {
                    let n = item.as_float().ok_or_else(type_err)?;
                    if n.is_nan() {
                        return Err(RuntimeError::ArithmeticError {
                            name: name.into(),
                            reason: "comparison with NaN".into(),
                        });
                    }
                    best = Some(match best {
                        None => n,
                        Some(b) if pick_max => b.max(n),
                        Some(b) => b.min(n),
                    });
                }
                Ok(Rc::new(Value::Float(
                    best.expect("items is non-empty").into(),
                )))
            }
            Some(_) => Err(type_err()),
        }
    })
}

fn min() -> NativeFn {
    extremum("min", false).with_doc(
        "\
(min N...)
(min NS)   ;; NS a single array or cons list of numbers

Returns the smallest of the given numbers, or the smallest element
of a single array or cons list of numbers. All elements must share
one numeric type (all ints or all floats).

Errors when the input is empty or a NaN is involved.

See also: (max), (clamp VAL LOW HIGH)."
            .into(),
    )
}

fn max() -> NativeFn {
    extremum("max", true).with_doc(
        "\
(max N...)
(max NS)   ;; NS a single array or cons list of numbers

Returns the largest of the given numbers, or the largest element
of a single array or cons list of numbers. All elements must share
one numeric type (all ints or all floats).

Errors when the input is empty or a NaN is involved.

See also: (min), (clamp VAL LOW HIGH)."
            .into(),
    )
}

fn clamp() -> NativeFn {
    NativeFn::pure("clamp".into(), 3, |args| {
        if let (Some(val), Some(low), Some(high)) = (
            i64::from_value(&args[0]),
            i64::from_value(&args[1]),
            i64::from_value(&args[2]),
        ) {
            if low > high {
                return Err(RuntimeError::ArithmeticError {
                    name: "clamp".into(),
                    reason: "low limit greater than high limit".into(),
                });
            }
            let res = val.clamp(low, high);
            return Ok(Rc::new(res.into()));
        }

        if let (Some(val), Some(low), Some(high)) = (
            f64::from_value(&args[0]),
            f64::from_value(&args[1]),
            f64::from_value(&args[2]),
        ) {
            if val.is_nan() || low.is_nan() || high.is_nan() {
                return Err(RuntimeError::ArithmeticError {
                    name: "clamp".into(),
                    reason: "comparison with NaN".into(),
                });
            }
            if low > high {
                return Err(RuntimeError::ArithmeticError {
                    name: "clamp".into(),
                    reason: "low limit greater than high limit".into(),
                });
            }
            let res = val.clamp(low, high);
            return Ok(Rc::new(res.into()));
        }

        Err(RuntimeError::TypeMismatch {
            name: "clamp".into(),
            expected: "int*int*int or float*float*float".into(),
            got: "other".into(),
        })
    })
    .with_doc(
        "\
(clamp VAL LOW HIGH)

Clamps VAL into the inclusive range [LOW, HIGH]. All three
arguments must share one numeric type (all ints or all floats).

Errors when LOW > HIGH or any float is NaN.

See also: (min), (max)."
            .into(),
    )
}

fn int_of() -> NativeFn {
    let name: Rc<str> = "int-of".into();
    NativeFn::pure(name.clone(), 1, move |args| {
        if let Some(n) = args[0].as_int() {
            return Ok(Rc::new(n.into()));
        }
        if let Some(f) = args[0].as_float() {
            let r = f.round_ties_even();
            if r.is_nan() {
                return Err(RuntimeError::ArithmeticError {
                    name: name.clone(),
                    reason: "NaN has no int value".into(),
                });
            }
            // [i64::MIN, 2^63) is exactly the f64 range that casts losslessly;
            // `i64::MAX as f64` rounds up to 2^63, so the comparison is strict.
            if r < i64::MIN as f64 || r >= i64::MAX as f64 {
                return Err(RuntimeError::ArithmeticError {
                    name: name.clone(),
                    reason: "integer overflow".into(),
                });
            }
            return Ok(Rc::new((r as i64).into()));
        }
        if let Some(s) = args[0].as_str() {
            return match i64::from_str(&s) {
                Ok(n) => Ok(Rc::new(n.into())),
                Err(e) => Err(RuntimeError::ParseError {
                    name: name.clone(),
                    reason: e.to_string().into(),
                }),
            };
        }
        Err(RuntimeError::type_mismatch(
            &name,
            "int or float or str",
            &args[0],
        ))
    })
    .with_doc(
        "\
(int-of VAL)

Converts VAL to an int: a float is rounded to the nearest int
(ties to even), a str is parsed as an int, and an int is returned
unchanged.

Errors when VAL is any other type, when a str fails to parse, or
when a float is NaN or out of int range.

See also: (float-of VAL), (str->int S)."
            .into(),
    )
}

fn float_of() -> NativeFn {
    let name: Rc<str> = "float-of".into();
    NativeFn::pure(name.clone(), 1, move |args| {
        if let Some(f) = args[0].as_float() {
            return Ok(Rc::new(f.into()));
        }
        if let Some(n) = args[0].as_int() {
            return Ok(Rc::new((n as f64).into()));
        }
        if let Some(s) = args[0].as_str() {
            return match f64::from_str(&s) {
                Ok(f) => Ok(Rc::new(f.into())),
                Err(e) => Err(RuntimeError::ParseError {
                    name: name.clone(),
                    reason: e.to_string().into(),
                }),
            };
        }
        Err(RuntimeError::type_mismatch(
            &name,
            "int or float or str",
            &args[0],
        ))
    })
    .with_doc(
        "\
(float-of VAL)

Converts VAL to a float: an int is converted (rounding to the
nearest float when it has no exact representation), a str is
parsed as a float, and a float is returned unchanged.

Errors when VAL is any other type or when a str fails to parse.

See also: (int-of VAL)."
            .into(),
    )
}

/// Attempts `op` for the numeric type `N`. Returns `Ok(None)` if the first
/// argument isn't an `N` (so the caller can try the other type), `Err` if the
/// first is an `N` but the second isn't, or if `op` itself fails.
fn try_binop<N, T, F>(
    name: &'static str,
    args: &[Rc<Value>],
    op: &F,
) -> Result<Option<Rc<Value>>, RuntimeError>
where
    N: Numeric,
    T: Into<Value>,
    F: Fn(N, N) -> Result<T, &'static str>,
{
    let Some(a) = N::from_value(&args[0]) else {
        return Ok(None);
    };
    let Some(b) = N::from_value(&args[1]) else {
        return Err(RuntimeError::TypeMismatch {
            name: name.into(),
            expected: format!("{0}*{0}", N::TYPE_NAME).into(),
            got: format!("{}*other", N::TYPE_NAME).into(),
        });
    };
    match op(a, b) {
        Ok(v) => Ok(Some(Rc::new(v.into()))),
        Err(reason) => Err(RuntimeError::ArithmeticError {
            name: name.into(),
            reason: reason.into(),
        }),
    }
}

/// Builds a binary builtin from an integer and a float implementation. The
/// returned function dispatches to `int_op` for two ints or `float_op` for two
/// floats, erroring on any other argument types. Arity 2 is enforced by
/// [`NativeFn::call`].
fn binop<TI, TF, FI, FF>(name: &'static str, int_op: FI, float_op: FF) -> NativeFn
where
    TI: Into<Value>,
    TF: Into<Value>,
    FI: Fn(i64, i64) -> Result<TI, &'static str> + 'static,
    FF: Fn(f64, f64) -> Result<TF, &'static str> + 'static,
{
    NativeFn::pure(name.into(), 2, move |args| {
        if let Some(v) = try_binop::<i64, _, _>(name, args, &int_op)? {
            return Ok(v);
        }
        if let Some(v) = try_binop::<f64, _, _>(name, args, &float_op)? {
            return Ok(v);
        }
        Err(RuntimeError::TypeMismatch {
            name: name.into(),
            expected: format!("int*int or float*float (in {})", name).into(),
            got: "other".into(),
        })
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

    // ----- comparison operators -----

    #[test]
    fn gt() {
        assert_eq!(*run_ok("(> 2 1)"), Value::Int(1));
        assert_eq!(*run_ok("(> 1 2)"), Value::Int(0));
        assert_eq!(*run_ok("(> 2 2)"), Value::Int(0));
    }

    #[test]
    fn gte() {
        assert_eq!(*run_ok("(>= 2 1)"), Value::Int(1));
        assert_eq!(*run_ok("(>= 2 2)"), Value::Int(1));
        assert_eq!(*run_ok("(>= 1 2)"), Value::Int(0));
    }

    #[test]
    fn lt() {
        assert_eq!(*run_ok("(< 1 2)"), Value::Int(1));
        assert_eq!(*run_ok("(< 2 1)"), Value::Int(0));
        assert_eq!(*run_ok("(< 2 2)"), Value::Int(0));
    }

    #[test]
    fn lte() {
        assert_eq!(*run_ok("(<= 1 2)"), Value::Int(1));
        assert_eq!(*run_ok("(<= 2 2)"), Value::Int(1));
        assert_eq!(*run_ok("(<= 3 2)"), Value::Int(0));
    }

    #[test]
    fn comparisons_work_on_floats() {
        assert_eq!(*run_ok("(< 1.5 2.5)"), Value::Int(1));
        assert_eq!(*run_ok("(>= 2.5 2.5)"), Value::Int(1));
        assert_eq!(*run_ok("(<= 3.5 2.5)"), Value::Int(0));
    }

    // ----- arithmetic that must not panic the interpreter -----

    #[test]
    fn integer_division_by_zero_is_error() {
        assert!(matches!(
            run("(/ 1 0)"),
            Err(RizzError::RuntimeError(
                RuntimeError::ArithmeticError { .. }
            ))
        ));
    }

    #[test]
    fn integer_overflow_is_error() {
        assert!(matches!(
            run("(+ 9223372036854775807 1)"),
            Err(RizzError::RuntimeError(
                RuntimeError::ArithmeticError { .. }
            ))
        ));
        assert!(matches!(
            run("(* 9223372036854775807 9223372036854775807)"),
            Err(RizzError::RuntimeError(
                RuntimeError::ArithmeticError { .. }
            ))
        ));
    }

    #[test]
    fn cmp_with_nan_is_error() {
        // 0.0 / 0.0 is NaN; comparing it must error rather than panic.
        assert!(matches!(
            run("(cmp (/ 0.0 0.0) 1.0)"),
            Err(RizzError::RuntimeError(
                RuntimeError::ArithmeticError { .. }
            ))
        ));
    }

    #[test]
    fn min_and_max() {
        // Ints
        assert_eq!(*run_ok("(min 10 20)"), Value::Int(10));
        assert_eq!(*run_ok("(min 20 10)"), Value::Int(10));
        assert_eq!(*run_ok("(max 10 20)"), Value::Int(20));
        assert_eq!(*run_ok("(max 20 10)"), Value::Int(20));

        // Floats
        assert_eq!(*run_ok("(min 1.5 2.5)"), Value::Float(1.5.into()));
        assert_eq!(*run_ok("(max 1.5 2.5)"), Value::Float(2.5.into()));

        // Mismatched types should error
        assert!(run("(min 10 2.5)").is_err());
        assert!(run("(max 1.5 10)").is_err());

        // variadics
        assert_eq!(*run_ok("(max 1 2 4 51 10)"), Value::Int(51));
        assert_eq!(*run_ok("(max [1 2 4 51 10])"), Value::Int(51));
        assert_eq!(*run_ok("(max '(1 2 4 51 10))"), Value::Int(51));
        assert!(run("(max '(1 2) 4 51 10))").is_err());
        assert!(run("(max [123] 4 51 10))").is_err());
        assert!(run("(max 4 51 [123] 10))").is_err());
        assert!(run("(max 4 51 '(123) 10))").is_err());
        assert!(run("(max 4 51 0.2 10))").is_err());
        assert_eq!(*run_ok("(min 1 2 4 51 10)"), Value::Int(1));
        assert_eq!(*run_ok("(min [1 2 4 51 10])"), Value::Int(1));
        assert_eq!(*run_ok("(min '(1 2 4 51 10))"), Value::Int(1));
        assert!(run("(min '(1 2) 4 51 10))").is_err());
        assert!(run("(min [123] 4 51 10))").is_err());
        assert!(run("(min 4 51 [123] 10))").is_err());
        assert!(run("(min 4 51 '(123) 10))").is_err());
        assert!(run("(min 4 51 0.2 10))").is_err());
    }

    #[test]
    fn mod_op() {
        // Least nonnegative remainder, regardless of operand signs.
        assert_eq!(*run_ok("(mod 7 3)"), Value::Int(1));
        assert_eq!(*run_ok("(mod -7 3)"), Value::Int(2));
        assert_eq!(*run_ok("(mod 7 -3)"), Value::Int(1));
        assert_eq!(*run_ok("(mod -7.5 3.0)"), Value::Float(1.5.into()));

        assert!(run("(mod 1 0)").is_err());
        assert!(run("(mod 1 2.0)").is_err());
    }

    #[test]
    fn int_of_op() {
        assert_eq!(*run_ok("(int-of 5)"), Value::Int(5));
        assert_eq!(*run_ok("(int-of \"42\")"), Value::Int(42));
        assert_eq!(*run_ok("(int-of 2.5)"), Value::Int(2)); // ties to even
        assert_eq!(*run_ok("(int-of 3.5)"), Value::Int(4));
        assert_eq!(*run_ok("(int-of -2.5)"), Value::Int(-2));
        assert_eq!(*run_ok("(int-of (ref 2.4))"), Value::Int(2));

        assert!(run("(int-of \"4.2\")").is_err());
        assert!(run("(int-of (/ 0.0 0.0))").is_err()); // NaN
        assert!(run("(int-of (/ 1.0 0.0))").is_err()); // out of int range
        assert!(run("(int-of [1])").is_err());
    }

    #[test]
    fn float_of_op() {
        assert_eq!(*run_ok("(float-of 2)"), Value::Float(2.0.into()));
        assert_eq!(*run_ok("(float-of 2.5)"), Value::Float(2.5.into()));
        assert_eq!(*run_ok("(float-of \"2.5\")"), Value::Float(2.5.into()));
        assert_eq!(*run_ok("(float-of (ref 2))"), Value::Float(2.0.into()));

        assert!(run("(float-of \"abc\")").is_err());
        assert!(run("(float-of [1])").is_err());
    }

    #[test]
    fn clamp_op() {
        // Ints
        assert_eq!(*run_ok("(clamp 5 1 10)"), Value::Int(5));
        assert_eq!(*run_ok("(clamp 0 1 10)"), Value::Int(1));
        assert_eq!(*run_ok("(clamp 15 1 10)"), Value::Int(10));

        // Floats
        assert_eq!(*run_ok("(clamp 5.5 1.5 10.5)"), Value::Float(5.5.into()));
        assert_eq!(*run_ok("(clamp 0.5 1.5 10.5)"), Value::Float(1.5.into()));
        assert_eq!(*run_ok("(clamp 15.5 1.5 10.5)"), Value::Float(10.5.into()));

        // Low limit > high limit should error
        assert!(run("(clamp 5 10 1)").is_err());
        assert!(run("(clamp 5.5 10.5 1.5)").is_err());

        // Mismatched types should error
        assert!(run("(clamp 5 1.5 10)").is_err());
    }
}
