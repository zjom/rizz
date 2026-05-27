use crate::evaluator::{Env, EvaluatorError, Value};
use std::rc::Rc;

pub fn eval(form: Rc<Value>, ctx: &Env) -> Result<Rc<Value>, EvaluatorError> {
    match &*form {
        Value::Int(_) | Value::Unit | Value::Str(_) | Value::Float(_) | Value::BuiltinFn(_) => {
            Ok(form.clone())
        }
        Value::Ident(ident) => {
            let f = ctx
                .get(ident)
                .ok_or(EvaluatorError::UnknownIdent(ident.clone()))?;

            Ok(f.clone())
        }
        Value::Cons { head, tail } => {
            let args = Value::iter(tail)
                .map(|v| eval(v.clone(), ctx))
                .collect::<Result<Vec<Rc<Value>>, EvaluatorError>>()?;
            let callable = eval(head.clone(), ctx)?;
            match &*callable {
                Value::BuiltinFn(f) => f(&args, ctx),
                Value::Closure { params, body, env } => eval_closure(&args, params, body, env),
                Value::Int(_) | Value::Unit | Value::Str(_) | Value::Float(_) => Ok(callable),
                _ => Err(EvaluatorError::NotCallable { value: callable }),
            }
        }
        Value::Closure { params, body, env } => eval_closure(&[], params, body, env),
    }
}

fn eval_closure(
    args: &[Rc<Value>],
    params: &[Rc<str>],
    body: &Rc<Value>,
    env: &Env,
) -> Result<Rc<Value>, EvaluatorError> {
    if params.len() != args.len() {
        return Err(EvaluatorError::ArityMismatch {
            expected: params.len(),
            got: args.len(),
        });
    }

    let mut call_env = env.clone();
    for (ident, arg) in params.iter().zip(args) {
        call_env.insert(ident.clone(), arg.clone());
    }
    eval(body.clone(), &call_env)
}
