use crate::{
    Env,
    runtime::{NativeFn, Value},
};
use std::rc::Rc;

pub fn env() -> Env {
    Env::of_builtins(vec![("typeof", typeof_())])
}

fn typeof_() -> NativeFn {
    NativeFn::pure("typeof".into(), 1, |args| {
        Ok(Rc::new(Value::Ident(Value::type_name(&args[0]).into())))
    })
}
