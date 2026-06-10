//! The standard environment of builtin functions.
//!
//! Each submodule contributes one group of builtins as a free-standing
//! [`Env`]. [`env()`] unions them together and then folds in the
//! rizz-defined macros from `_.rz` (`cond`, `match`, `unless`, `for`, `loop`, `while`,
//! `and`, `or`, `compose`, `pipe`). The resulting env is what
//! [`crate::Runtime::new`] and [`crate::parse_and_run`] start from.
//!
//! | Submodule        | Provides                                                    |
//! | ---------------- | ----------------------------------------------------------- |
//! | [`numbers`]      | `+ - * /`, `cmp`, `< <= > >=`, `min`, `max`, `clamp`        |
//! | [`eq`]           | `= eq`, `!= neq`, `! not`                                   |
//! | [`map`]          | `put`, `put!`, `keys`, `values`, `del`, `del!`              |
//! | [`collections`]  | `len`, `get`, `concat`, `slice`, `fmap`, `filter`, `reduce` |
//! | [`mod@str`]      | `to-str`, `str-upper`, `str-split`, `str-join`, …           |
//! | [`mod@array`]    | `push`, `pop`, `range`, `array-of`, `array-from`            |
//! | [`cons`]         | `cons`, `car`, `cdr`, `car!`, `cdr!`                        |
//! | [`ref_`]         | `ref`, `deref`, `set!`                                      |
//! | [`meta`]         | `typeof`, `show`, `id`                                      |
//!
//! To add custom Rust builtins without losing the prelude, use [`install`]:
//!
//! ```
//! use rizz::{Env, Runtime, runtime::{NativeFn, Value}};
//! use std::rc::Rc;
//!
//! let f = NativeFn::pure("answer".into(), 0, |_| Ok(Rc::new(Value::Int(42))));
//! let extra = Env::new().update("answer".into(), Rc::new(Value::NativeFn(Rc::new(f))));
//! let env = rizz::prelude::install(extra);
//! let mut rt = Runtime::with_env(env);
//! assert_eq!(*rt.eval(b"(answer)".as_ref()).unwrap(), Value::Int(42));
//! ```

pub mod array;
pub mod collections;
pub mod cons;
pub mod eq;
pub mod map;
pub mod meta;
pub mod numbers;
pub mod ref_;
pub mod str;

use std::io::Cursor;

use crate::runtime::Env;

thread_local! {
    /// The prelude built once per thread: constructing it parses and
    /// evaluates `_.rz`, which is too much work to repeat on every
    /// [`env()`] call (e.g. each [`crate::parse_and_run`]). `Env` values
    /// are `Rc`-backed and not `Send`, so a per-thread cache is the
    /// natural granularity.
    static PRELUDE: Env = build_env();
}

/// Build a fresh default environment.
///
/// The env contains every Rust-implemented builtin from this module's
/// submodules plus the rizz-defined macros from `_.rz` (`cond`, `match`,
/// `unless`, `for`, `loop`, `while`, `and`, `or`, `compose`, `pipe`). This is the
/// env [`crate::Runtime::new`] starts from.
///
/// The prelude is built once per thread and cloned out on each call —
/// cloning an [`Env`] is cheap (it's backed by [`im::HashMap`]), and the
/// returned env is independent: updates to it never affect the cache.
pub fn env() -> Env {
    PRELUDE.with(Env::clone)
}

fn build_env() -> Env {
    let builtins = Env::new()
        .union(numbers::env())
        .union(eq::env())
        .union(map::env())
        .union(collections::env())
        .union(str::env())
        .union(array::env())
        .union(cons::env())
        .union(ref_::env())
        .union(meta::env());

    let (_, env) = crate::parse_and_run_with_env(Cursor::new(include_bytes!("./_.rz")), &builtins)
        .expect("prelude shouldn't fail");
    env
}

/// Merge `e` into a fresh prelude env. On key collision **`e` wins** (see
/// [`Env::union`]), so host builtins can both add new names and override
/// standard ones — which is what "install" should mean. To get the old
/// prelude-wins behavior, write `prelude::env().union(e)` yourself.
pub fn install(e: Env) -> Env {
    e.union(env())
}
