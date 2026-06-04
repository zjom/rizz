//! Arithmetic and comparison builtins.
//!
//! Every operator here is binary and works on two ints or two floats (never a
//! mix). They share the generic `binop` machinery, which dispatches on the
//! argument type and turns Rust-level faults (overflow, divide-by-zero, NaN)
//! into [`RuntimeError::ArithmeticError`]. Comparisons return `1` for true
//! and `0` for false.

use crate::runtime::Numeric;
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

/// The arithmetic/comparison builtins: `+ - * /`, `cmp`, and `> >= < <=`.
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

/// `ctx` extended with this module's builtins.
pub fn install(ctx: Env) -> Env {
    env().union(ctx)
}

fn add() -> NativeFn {
    binop(
        "add",
        |a, b| a.checked_add(b).ok_or("integer overflow"),
        |a, b| Ok(a + b),
    )
}
fn sub() -> NativeFn {
    binop(
        "sub",
        |a, b| a.checked_sub(b).ok_or("integer overflow"),
        |a, b| Ok(a - b),
    )
}

fn mul() -> NativeFn {
    binop(
        "mul",
        |a, b| a.checked_mul(b).ok_or("integer overflow"),
        |a, b| Ok(a * b),
    )
}

fn div() -> NativeFn {
    binop(
        "div",
        |a, b| a.checked_div(b).ok_or("division by zero"),
        |a, b| Ok(a / b),
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
}

fn gt() -> NativeFn {
    binop("gt", |a, b| Ok(a > b), |a, b| Ok(a > b))
}

fn gte() -> NativeFn {
    binop("gte", |a, b| Ok(a >= b), |a, b| Ok(a >= b))
}

fn lt() -> NativeFn {
    binop("lt", |a, b| Ok(a < b), |a, b| Ok(a < b))
}

fn lte() -> NativeFn {
    binop("lte", |a, b| Ok(a <= b), |a, b| Ok(a <= b))
}

fn min() -> NativeFn {
    binop(
        "min",
        |a, b| Ok(a.min(b)),
        |a, b| {
            if a.is_nan() || b.is_nan() {
                Err("comparison with NaN")
            } else {
                Ok(a.min(b))
            }
        },
    )
}

fn max() -> NativeFn {
    binop(
        "max",
        |a, b| Ok(a.max(b)),
        |a, b| {
            if a.is_nan() || b.is_nan() {
                Err("comparison with NaN")
            } else {
                Ok(a.max(b))
            }
        },
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
