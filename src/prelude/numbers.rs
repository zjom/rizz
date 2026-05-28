//! Arithmetic and comparison builtins.
//!
//! Every operator here is binary and works on two ints or two floats (never a
//! mix). They share the generic `binop` machinery, which dispatches on the
//! argument type and turns Rust-level faults (overflow, divide-by-zero, NaN)
//! into [`EvaluatorError::ArithmeticError`]. Comparisons return `1` for true
//! and `0` for false.

use crate::evaluator::Numeric;
use std::rc::Rc;

use crate::evaluator::{BuiltinFn, Env, EvaluatorError, Value};

/// The arithmetic/comparison builtins: `+ - * /`, `cmp`, and `> >= < <=`.
pub fn env() -> Env {
    Env::of_builtins(vec![
        ("+", add()),
        ("-", sub()),
        ("*", mul()),
        ("/", div()),
        ("cmp", cmp()),
        (">", gt()),
        (">=", gte()),
        ("<", lt()),
        ("<=", lte()),
    ])
}

/// `ctx` extended with this module's builtins.
pub fn install(ctx: Env) -> Env {
    env().union(ctx)
}

fn add() -> BuiltinFn {
    binop(
        "add",
        |a, b| a.checked_add(b).ok_or("integer overflow"),
        |a, b| Ok(a + b),
    )
}
fn sub() -> BuiltinFn {
    binop(
        "sub",
        |a, b| a.checked_sub(b).ok_or("integer overflow"),
        |a, b| Ok(a - b),
    )
}

fn mul() -> BuiltinFn {
    binop(
        "mul",
        |a, b| a.checked_mul(b).ok_or("integer overflow"),
        |a, b| Ok(a * b),
    )
}

fn div() -> BuiltinFn {
    binop(
        "div",
        |a, b| a.checked_div(b).ok_or("division by zero"),
        |a, b| Ok(a / b),
    )
}

fn cmp() -> BuiltinFn {
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

fn gt() -> BuiltinFn {
    binop("gt", |a, b| Ok(a > b), |a, b| Ok(a > b))
}

fn gte() -> BuiltinFn {
    binop("gte", |a, b| Ok(a >= b), |a, b| Ok(a >= b))
}

fn lt() -> BuiltinFn {
    binop("lt", |a, b| Ok(a < b), |a, b| Ok(a < b))
}

fn lte() -> BuiltinFn {
    binop("lte", |a, b| Ok(a <= b), |a, b| Ok(a <= b))
}

/// Attempts `op` for the numeric type `N`. Returns `Ok(None)` if the first
/// argument isn't an `N` (so the caller can try the other type), `Err` if the
/// first is an `N` but the second isn't, or if `op` itself fails.
fn try_binop<N, T, F>(
    name: &'static str,
    args: &[Rc<Value>],
    op: &F,
) -> Result<Option<Rc<Value>>, EvaluatorError>
where
    N: Numeric,
    T: Into<Value>,
    F: Fn(N, N) -> Result<T, &'static str>,
{
    let Some(a) = N::from_value(&args[0]) else {
        return Ok(None);
    };
    let Some(b) = N::from_value(&args[1]) else {
        return Err(EvaluatorError::TypeMismatch {
            name: name.into(),
            expected: format!("{0}*{0}", N::TYPE_NAME).into(),
            got: format!("{}*other", N::TYPE_NAME).into(),
        });
    };
    match op(a, b) {
        Ok(v) => Ok(Some(Rc::new(v.into()))),
        Err(reason) => Err(EvaluatorError::ArithmeticError {
            name: name.into(),
            reason: reason.into(),
        }),
    }
}

/// Builds a binary builtin from an integer and a float implementation. The
/// returned function enforces arity 2, then dispatches to `int_op` for two
/// ints or `float_op` for two floats, erroring on any other argument types.
fn binop<TI, TF, FI, FF>(name: &'static str, int_op: FI, float_op: FF) -> BuiltinFn
where
    TI: Into<Value>,
    TF: Into<Value>,
    FI: Fn(i64, i64) -> Result<TI, &'static str> + 'static,
    FF: Fn(f64, f64) -> Result<TF, &'static str> + 'static,
{
    Rc::new(move |args, env| {
        if args.len() != 2 {
            return Err(EvaluatorError::ArityMismatch {
                name: name.into(),
                expected: 2,
                got: args.len(),
            });
        }
        if let Some(v) = try_binop::<i64, _, _>(name, args, &int_op)? {
            return Ok((v, env.clone()));
        }
        if let Some(v) = try_binop::<f64, _, _>(name, args, &float_op)? {
            return Ok((v, env.clone()));
        }
        Err(EvaluatorError::TypeMismatch {
            name: name.into(),
            expected: format!("int*int or float*float (in {})", name).into(),
            got: "other".into(),
        })
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
            Err(RispError::RuntimeError(EvaluatorError::ArithmeticError { .. }))
        ));
    }

    #[test]
    fn integer_overflow_is_error() {
        assert!(matches!(
            run("(+ 9223372036854775807 1)"),
            Err(RispError::RuntimeError(EvaluatorError::ArithmeticError { .. }))
        ));
        assert!(matches!(
            run("(* 9223372036854775807 9223372036854775807)"),
            Err(RispError::RuntimeError(EvaluatorError::ArithmeticError { .. }))
        ));
    }

    #[test]
    fn cmp_with_nan_is_error() {
        // 0.0 / 0.0 is NaN; comparing it must error rather than panic.
        assert!(matches!(
            run("(cmp (/ 0.0 0.0) 1.0)"),
            Err(RispError::RuntimeError(EvaluatorError::ArithmeticError { .. }))
        ));
    }
}
