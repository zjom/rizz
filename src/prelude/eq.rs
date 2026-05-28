use crate::evaluator::EvaluatorError;
use crate::evaluator::{BuiltinFn, Env};
use std::rc::Rc;

pub fn env() -> Env {
    Env::of_builtins(vec![("=", eq())])
}

pub fn install(ctx: Env) -> Env {
    env().union(ctx)
}

fn eq() -> BuiltinFn {
    let name = "eq";
    Rc::new(move |args, env| {
        if args.len() != 2 {
            return Err(EvaluatorError::ArityMismatch {
                name: name.into(),
                expected: 2,
                got: args.len(),
            });
        }
        let v = Rc::new((args[0] == args[1]).into());
        Ok((v, env.clone()))
    })
}
