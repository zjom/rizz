use crate::evaluator::Numeric;
use std::rc::Rc;

use crate::evaluator::{BuiltinFn, Env, EvaluatorError, Value};

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
pub fn install(ctx: Env) -> Env {
    env().union(ctx)
}

fn add() -> BuiltinFn {
    binop("add", |a, b| a + b, |a, b| a + b)
}
fn sub() -> BuiltinFn {
    binop("sub", |a, b| a - b, |a, b| a - b)
}

fn mul() -> BuiltinFn {
    binop("mul", |a, b| a * b, |a, b| a * b)
}

fn div() -> BuiltinFn {
    binop("div", |a, b| a / b, |a, b| a / b)
}

fn cmp() -> BuiltinFn {
    binop(
        "cmp",
        |a, b| match a.cmp(&b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Equal => 0,
        },
        |a, b| match a.partial_cmp(&b).expect("NaN's are unexpressable") {
            std::cmp::Ordering::Less => -1.,
            std::cmp::Ordering::Greater => 1.,
            std::cmp::Ordering::Equal => 0.,
        },
    )
}

fn gt() -> BuiltinFn {
    binop("gt", |a, b| a > b, |a, b| a > b)
}

fn gte() -> BuiltinFn {
    binop("gte", |a, b| a > b, |a, b| a > b)
}

fn lt() -> BuiltinFn {
    binop("lt", |a, b| a > b, |a, b| a > b)
}

fn lte() -> BuiltinFn {
    binop("lte", |a, b| a > b, |a, b| a > b)
}

fn try_binop<N, T, F>(
    name: &'static str,
    args: &[Rc<Value>],
    op: &F,
) -> Result<Option<Rc<Value>>, EvaluatorError>
where
    N: Numeric,
    T: Into<Value>,
    F: Fn(N, N) -> T,
{
    let Some(a) = N::from_value(&args[0]) else {
        return Ok(None);
    };
    match N::from_value(&args[1]) {
        Some(b) => Ok(Some(Rc::new(op(a, b).into()))),
        None => Err(EvaluatorError::TypeMismatch {
            name: name.into(),
            expected: format!("{0}*{0}", N::TYPE_NAME).into(),
            got: format!("{}*other", N::TYPE_NAME).into(),
        }),
    }
}

fn binop<TI, TF, FI, FF>(name: &'static str, int_op: FI, float_op: FF) -> BuiltinFn
where
    TI: Into<Value>,
    TF: Into<Value>,
    FI: Fn(i64, i64) -> TI + 'static,
    FF: Fn(f64, f64) -> TF + 'static,
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
