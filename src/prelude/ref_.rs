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
    .with_doc(
        "(ref v): wraps v in a mutable cell. Use deref to read and set! (or put!, push!, \
         car!, cdr!, etc.) to write."
            .into(),
    )
}

fn deref() -> NativeFn {
    NativeFn::pure("deref".into(), 1, |args| match &*args[0] {
        Value::Ref(cell) => Ok(Rc::new(cell.borrow().clone())),
        other => Err(RuntimeError::type_mismatch("deref", "ref", other)),
    })
    .with_doc("(deref r): the value currently held in the ref r. Errors if r is not a ref.".into())
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
    .with_doc(
        "(set! r v): overwrites the value held in ref r with v and returns v. \
         Errors if r is not a ref."
            .into(),
    )
}
