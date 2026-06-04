use std::rc::Rc;
use std::{cell::RefCell, ops::Deref};

use crate::{
    Env,
    runtime::{NativeFn, RuntimeError, Value},
};

pub fn env() -> Env {
    Env::of_builtins(vec![("ref", _ref()), ("deref", deref()), ("set!", set())])
}

fn _ref() -> NativeFn {
    NativeFn::pure("ref".into(), 1, |args| {
        Ok(Rc::new(Value::Ref(Rc::new(RefCell::new(
            args[0].deref().to_owned(),
        )))))
    })
}

fn deref() -> NativeFn {
    NativeFn::pure("deref".into(), 1, |args| match &*args[0] {
        Value::Ref(cell) => Ok(Rc::new(cell.borrow().clone())),
        other => Err(RuntimeError::type_mismatch("deref", "ref", other)),
    })
}

fn set() -> NativeFn {
    NativeFn::pure("set!".into(), 2, |args| match &*args[0] {
        Value::Ref(cell) => {
            let new = args[1].deref().to_owned();
            *cell.borrow_mut() = new.clone();
            Ok(Rc::new(new))
        }
        other => Err(RuntimeError::type_mismatch("set!", "ref", other)),
    })
}
