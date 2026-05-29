use im::HashMap;

use crate::runtime::{RuntimeError, Value};
use std::rc::Rc;

use crate::runtime::{Env, NativeFn};

pub fn env() -> Env {
    Env::of_builtins(vec![("put", put())])
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
