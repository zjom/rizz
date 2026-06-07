//! Ref builtins: `ref`, `deref`, `set!`. A [`Value::Ref`] is the only
//! mutable value kind in rizz — every other value is persistent — so refs
//! are the path to shared, in-place state.
//!
//! Two bindings of the same ref share one cell, and closures that capture
//! a ref see writes made through any alias. The unsuffixed builtins here
//! cover allocate / read / write; in-place collection updates (`push!`,
//! `put!`, `car!`, …) live alongside their non-mutating counterparts in
//! [`array`], [`map`], and [`cons`].
//!
//! See the language spec for the full ref semantics, including the
//! transparent deref of numeric / comparison ops and head-position
//! callables.
//!
//! [`array`]: crate::prelude::array
//! [`map`]: crate::prelude::map
//! [`cons`]: crate::prelude::cons

use std::rc::Rc;
use std::{cell::RefCell, ops::Deref};

use crate::{
    Env,
    runtime::{NativeFn, RuntimeError, Value},
};

/// All ref builtins: `ref`, `deref`, `set!`.
pub fn env() -> Env {
    Env::of_builtins(vec![("ref", _ref()), ("deref", deref()), ("set!", set())])
}

/// `(ref v)`: allocates a fresh ref cell initialized to `v`.
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

/// `(deref r)`: reads the current contents of `r`. Errors on non-ref.
fn deref() -> NativeFn {
    NativeFn::pure("deref".into(), 1, |args| match &*args[0] {
        Value::Ref(cell) => Ok(Rc::new(cell.borrow().clone())),
        other => Err(RuntimeError::type_mismatch("deref", "ref", other)),
    })
    .with_doc("(deref r): the value currently held in the ref r. Errors if r is not a ref.".into())
}

/// `(set! r v)`: overwrites `r` with `v`; returns `v`. Stores `v` verbatim —
/// `(set! r (ref x))` aliases `r` to a ref-of-ref, no implicit deref.
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
