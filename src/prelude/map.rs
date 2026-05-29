use im::HashMap;

use crate::runtime::{RuntimeError, Value};
use std::rc::Rc;

use crate::runtime::{Env, NativeFn};

pub fn env() -> Env {
    Env::of_builtins(vec![("get", get()), ("put", put())])
}

fn get() -> NativeFn {
    NativeFn::pure("get".into(), 2, |args| match &*args[0] {
        Value::Map(m) => match m.get(&args[1]) {
            Some(val) => Ok(val.clone()),
            None => Ok(Value::Unit.into()),
        },
        other => Err(RuntimeError::TypeMismatch {
            name: "get".into(),
            expected: Value::type_name(&Value::Map(HashMap::new())).into(),
            got: Value::type_name(other).into(),
        }),
    })
}

fn put() -> NativeFn {
    NativeFn::pure("put".into(), 3, |args| match &*args[0] {
        Value::Map(m) => {
            let m = m.update(args[1].clone(), args[2].clone());
            Ok(Rc::new(Value::Map(m)))
        }
        other => Err(RuntimeError::TypeMismatch {
            name: "put".into(),
            expected: Value::type_name(&Value::Map(HashMap::new())).into(),
            got: Value::type_name(other).into(),
        }),
    })
}
