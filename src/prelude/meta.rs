use crate::{
    Env,
    runtime::{NativeFn, Value},
};
use std::rc::Rc;

pub fn env() -> Env {
    Env::of_builtins(vec![("typeof", typeof_()), ("show", show())])
}

fn typeof_() -> NativeFn {
    NativeFn::pure("typeof".into(), 1, |args| {
        Ok(Rc::new(Value::Ident(Value::type_name(&args[0]).into())))
    })
}

/// `(show v)`: returns the doc string attached to a closure, macro, or native
/// fn at definition time (see the optional `(doc "...")` slot on binding
/// forms). Returns `()` when `v` carries no doc. Refs are peeled so
/// `(show (deref r))` and `(show r)` behave the same.
fn show() -> NativeFn {
    NativeFn::pure("show".into(), 1, |args| {
        Ok(match doc_of(&args[0]) {
            Some(s) => Rc::new(Value::Str(s)),
            None => Rc::new(Value::Unit),
        })
    })
}

fn doc_of(v: &Value) -> Option<Rc<str>> {
    match v {
        Value::Closure(c) | Value::Macro(c) => c.doc.clone(),
        Value::NativeFn(n) => n.doc(),
        Value::Ref(cell) => doc_of(&cell.borrow()),
        _ => None,
    }
}
