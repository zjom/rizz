//! The rizz runtime: values, environment, and the tree-walking interpreter.
//!
//! - [`Value`] — the universal runtime datatype. Doubles as the AST the
//!   interpreter walks (a `(+ 1 2)` form is a `Value::Cons` chain).
//! - [`Closure`] — a user-defined function: name, params, body, captured
//!   env. Wrapped in [`Value::Closure`] or [`Value::Macro`].
//! - [`Env`] — lexical bindings plus `(open ...)` context. Threaded
//!   through every evaluation step.
//! - [`NativeFn`] — a Rust function exposed to rizz, in one of four
//!   flavors (`Pure`, `WithEnv`, `Impure`, `Macro`). See the
//!   [`native`](self) module docs for guidance on which to use.
//! - [`eval`] — the heart of the interpreter. Dispatches special forms and
//!   function applications, returning `(value, env')`.
//! - [`apply`] — invokes a callable on already-evaluated args; used by
//!   higher-order builtins.
//! - [`Runtime`] — a stateful handle that owns an env across calls; what
//!   embedders and REPLs use.
//! - [`RuntimeError`] — every failure the runtime can raise.
//!
//! Submodules are private; their public items are re-exported here so
//! callers say `runtime::Value`, `runtime::eval`, etc.

mod env;
mod error;
mod eval;
mod native;
mod rt;
mod value;

pub use env::*;
pub use error::*;
pub use eval::*;
pub use native::*;
pub use rt::*;
pub use value::*;
