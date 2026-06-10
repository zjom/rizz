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
        "\
(ref V)

Wraps V in a mutable cell — the only mutable value kind in rizz.
Read it with (deref R); write it with (set! R V) or the in-place
collection ops (put!, push!, car!, ...).

See also: (deref R), (set! R V)."
            .into(),
    )
}

/// `(deref r)`: reads the current contents of `r`. Errors on non-ref.
fn deref() -> NativeFn {
    NativeFn::pure("deref".into(), 1, |args| match &*args[0] {
        Value::Ref(cell) => Ok(Rc::new(cell.borrow().clone())),
        other => Err(RuntimeError::type_mismatch("deref", "ref", other)),
    })
    .with_doc(
        "\
(deref R)

Returns the value currently held in the ref R.

See also: (ref V), (set! R V)."
            .into(),
    )
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
        "\
(set! R V)

Overwrites the value held in the ref R with V and returns V. V is
stored verbatim — (set! r (ref x)) stores a ref-of-ref, with no
implicit deref.

See also: (ref V), (deref R)."
            .into(),
    )
}
