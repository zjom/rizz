use im::HashMap;

use crate::runtime::{RuntimeError, Value};
use std::rc::Rc;

use crate::runtime::{BuiltinFn, Env};

pub fn env() -> Env {
    Env::of_builtins(vec![("get", get()), ("put", put())])
}

fn get() -> BuiltinFn {
    Rc::new(move |args, env| {
        let name: Rc<str> = "get".into();
        if args.len() != 2 {
            return Err(RuntimeError::ArityMismatch {
                name: name.clone(),
                expected: 2,
                got: args.len(),
            });
        }

        match &*args[0] {
            Value::Map(m) => match m.get(&args[1]) {
                Some(val) => Ok((val.clone(), env.clone())),
                None => Ok((Value::Unit.into(), env.clone())),
            },
            other => Err(RuntimeError::TypeMismatch {
                name,
                expected: Value::type_name(&Value::Map(HashMap::new())).into(),
                got: Value::type_name(other).into(),
            }),
        }
    })
}

fn put() -> BuiltinFn {
    Rc::new(move |args, env| {
        let name: Rc<str> = "put".into();
        if args.len() != 3 {
            return Err(RuntimeError::ArityMismatch {
                name: name.clone(),
                expected: 2,
                got: args.len(),
            });
        }

        match &*args[0] {
            Value::Map(m) => {
                let m = m.update(args[1].clone(), args[2].clone());
                Ok((Rc::new(Value::Map(m)), env.clone()))
            }
            other => Err(RuntimeError::TypeMismatch {
                name,
                expected: Value::type_name(&Value::Map(HashMap::new())).into(),
                got: Value::type_name(other).into(),
            }),
        }
    })
}
