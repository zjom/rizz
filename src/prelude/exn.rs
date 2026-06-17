//! Exception primitive: `raise`.
//!
//! `raise` is the one builtin that deliberately fails: it wraps its
//! argument in [`RuntimeError::Raised`], which the `?` operator threads up
//! through every `eval` frame until the nearest `(try ...)` special form
//! catches it (see [`crate::runtime::eval`]). The rest of the exception
//! surface — `(exception NAME)` constructors, `failwith`, `exn?`, and the
//! OCaml-style `try-with` matching macro — is defined in rizz in `_.rz` on
//! top of this primitive and the `try` special form.

use crate::{Env, RuntimeError, runtime::NativeFn};

/// The exception primitive: `raise`.
pub fn env() -> Env {
    Env::of_builtins(vec![("raise", raise())])
}

/// `(raise V)`: abort evaluation with `V` as the raised value.
fn raise() -> NativeFn {
    NativeFn::pure("raise".into(), 1, |args| {
        Err(RuntimeError::Raised {
            value: args[0].clone(),
        })
    })
    .with_doc(
        "\
(raise V)

Aborts evaluation, raising V as an exception that unwinds to the
nearest enclosing (try ...). V is any value; by convention it is a
tagged cons ('Name arg...) built by an (exception Name) constructor.
An uncaught raise aborts the program.

See also: (try BODY (catch VAR HANDLER...)), (exception NAME),
(failwith MSG)."
            .into(),
    )
}
