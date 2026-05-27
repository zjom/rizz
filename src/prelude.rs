use std::rc::Rc;

use crate::evaluator::{BuiltinFn, Env, EvaluatorError, Value};

pub fn env() -> Env {
    install(&Env::new())
}
pub fn install(ctx: &Env) -> Env {
    ctx.update("+".into(), Rc::new(Value::BuiltinFn(add())))
        .update("-".into(), Rc::new(Value::BuiltinFn(sub())))
}

fn add() -> BuiltinFn {
    Rc::new(|args, _env| {
        if args.len() != 2 {
            return Err(EvaluatorError::ArityMismatch {
                expected: 2,
                got: args.len(),
            });
        }

        match &*args[0] {
            Value::Int(a) => {
                if let Some(b) = to_int(&args[1]) {
                    Ok(Rc::new(Value::Int(a + b)))
                } else {
                    Err(EvaluatorError::TypeMismatch {
                        expected: "int*int".into(),
                        got: "int*other".into(),
                    })
                }
            }

            Value::Float(a) => {
                if let Some(b) = to_float(&args[1]) {
                    Ok(Rc::new(Value::Float(a + b)))
                } else {
                    Err(EvaluatorError::TypeMismatch {
                        expected: "float*float".into(),
                        got: "float*other".into(),
                    })
                }
            }

            _ => Err(EvaluatorError::TypeMismatch {
                expected: "int*int or float*float".into(),
                got: "other".into(),
            }),
        }
    })
}

fn sub() -> BuiltinFn {
    Rc::new(|args, _env| {
        if args.len() != 2 {
            return Err(EvaluatorError::ArityMismatch {
                expected: 2,
                got: args.len(),
            });
        }

        match &*args[0] {
            Value::Int(a) => {
                if let Some(b) = to_int(&args[1]) {
                    Ok(Rc::new(Value::Int(a - b)))
                } else {
                    Err(EvaluatorError::TypeMismatch {
                        expected: "int*int".into(),
                        got: "int*other".into(),
                    })
                }
            }

            Value::Float(a) => {
                if let Some(b) = to_float(&args[1]) {
                    Ok(Rc::new(Value::Float(a - b)))
                } else {
                    Err(EvaluatorError::TypeMismatch {
                        expected: "float*float".into(),
                        got: "float*other".into(),
                    })
                }
            }

            _ => Err(EvaluatorError::TypeMismatch {
                expected: "int*int or float*float".into(),
                got: "other".into(),
            }),
        }
    })
}

fn to_int(v: &Rc<Value>) -> Option<i64> {
    match &**v {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn to_float(v: &Rc<Value>) -> Option<f64> {
    match &**v {
        Value::Float(n) => Some(*n),
        _ => None,
    }
}
