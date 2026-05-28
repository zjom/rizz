//! The evaluator: runtime values and the tree-walking interpreter.
//!
//! - [`Value`] is the runtime datatype, with [`Closure`] and the [`Env`] of
//!   bindings it captures.
//! - [`eval`] walks a `Value` form, handling special forms (`let`, `fn`, `if`,
//!   `quote`, `quasi`) and function application.
//! - [`EvaluatorError`] is the failure type.
//!
//! Submodules are private; their public items are re-exported here so callers
//! use `evaluator::Value`, `evaluator::eval`, etc.

mod env;
mod error;
mod eval;
mod value;

pub use env::*;
pub use error::*;
pub use eval::*;
pub use value::*;
