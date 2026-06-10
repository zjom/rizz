//! # rizz — a small, embeddable Lisp interpreter
//!
//! `rizz` is a tree-walking interpreter for a dynamically typed Lisp, exposed
//! as a Rust library so it can be driven directly from host code or embedded
//! inside a larger application. The language itself is specified in
//! [`SPEC.md`](https://github.com/zjom/rizz/blob/main/SPEC.md); this page
//! focuses on **how to drive the library from Rust**.
//!
//! The crate is organized around three stages, each in its own module:
//!
//! | Stage    | Module          | Output                                |
//! | -------- | --------------- | ------------------------------------- |
//! | Parse    | [`parser`]      | [`parser::Sexp`] forms with positions |
//! | Evaluate | [`runtime`]     | [`runtime::Value`] + updated [`Env`]  |
//! | Builtins | [`prelude`]     | The default [`Env`]                   |
//!
//! Most users do not interact with the stages directly — the helpers
//! [`parse_and_run`] and [`Runtime`] wire them together for one-shot
//! evaluation and stateful sessions respectively.
//!
//! ---
//!
//! ## At a glance
//!
//! ```
//! use rizz::runtime::Value;
//!
//! // One-shot: parse, evaluate, take the final value.
//! let (value, _env) = rizz::parse_and_run(b"(+ 1 (* 2 3))".as_ref()).unwrap();
//! assert_eq!(*value, Value::Int(7));
//! ```
//!
//! Each top-level form is evaluated in order and bindings thread between them,
//! so later forms see earlier `let`/`fn` definitions:
//!
//! ```
//! use rizz::runtime::Value;
//!
//! let src = b"(let x 10) (+ x 5)";
//! let (value, _env) = rizz::parse_and_run(src.as_ref()).unwrap();
//! assert_eq!(*value, Value::Int(15));
//! ```
//!
//! ---
//!
//! ## Choosing an entry point
//!
//! | Use case                                          | Entry point                  |
//! | ------------------------------------------------- | ---------------------------- |
//! | Run a string, get the last value back             | [`parse_and_run`]            |
//! | Run a string with caller-supplied bindings        | [`parse_and_run_with_env`]   |
//! | Evaluate pre-parsed forms                         | [`eval_forms`]               |
//! | Repeated calls that share state (REPL, file load) | [`Runtime`]                  |
//! | Load and evaluate a `.rz` file                    | [`Runtime::eval_file`]       |
//! | Just parse, no evaluation                         | [`Parser`]                   |
//!
//! ### Stateful sessions with `Runtime`
//!
//! [`Runtime`] holds an [`Env`] that grows across calls, which is what a REPL
//! or a host that incrementally feeds the user input wants. It also pins a
//! **base env** used to seed every `(open ...)`d module — embedders can
//! install host-specific builtins via [`Runtime::with_env`] and have them
//! reach loaded modules transparently.
//!
//! ```
//! use rizz::{Runtime, runtime::Value};
//!
//! let mut rt = Runtime::new();
//! rt.eval(b"(let x 1)".as_ref()).unwrap();        // binds `x` in the session
//! rt.eval(b"(let y 2)".as_ref()).unwrap();        // adds `y`; `x` still visible
//! let v = rt.eval(b"(+ x y)".as_ref()).unwrap();  // sees both
//! assert_eq!(*v, Value::Int(3));
//! ```
//!
//! ### Loading files
//!
//! [`Runtime::eval_file`] reads a path, parses it, and anchors the runtime's
//! `base_dir` so that any `(open "...")` inside the file resolves relative to
//! its directory:
//!
//! ```no_run
//! use rizz::Runtime;
//!
//! let mut rt = Runtime::new();
//! let value = rt.eval_file("examples/main.rz").unwrap();
//! println!("{value}");
//! ```
//!
//! ### Per-form evaluation
//!
//! When the host has already parsed (or constructed) forms it can feed them
//! one at a time via [`Runtime::eval_form`]:
//!
//! ```
//! use rizz::{Parser, Runtime, runtime::Value};
//! use std::rc::Rc;
//!
//! let forms = Parser::new(b"(let n 21) (* n 2)".as_ref()).parse().unwrap();
//! let mut rt = Runtime::new();
//! let mut last = Rc::new(Value::Unit);
//! for form in forms {
//!     last = rt.eval_form(Rc::new(form.into())).unwrap();
//! }
//! assert_eq!(*last, Value::Int(42));
//! ```
//!
//! ---
//!
//! ## The pipeline in detail
//!
//! Source bytes travel through three stages:
//!
//! 1. **Parse.** [`Parser`] streams bytes from any [`std::io::Read`] and emits
//!    a `Vec<Sexp>` — one entry per top-level form. The parser tracks
//!    line/column [`Position`](parser::Position) so every [`ParseError`] can
//!    point at the offending byte. Identifiers are interned, so equal names
//!    share one `Rc<str>` allocation.
//!
//! 2. **Lower.** Each [`Sexp`] converts into a [`Value`] via
//!    `impl From<Sexp> for Value`. This is a
//!    structural rewrite: atoms become atom values, lists become `Cons`
//!    chains, `[...]` becomes `Value::Array`, `{...}` becomes `Value::Map`.
//!
//! 3. **Evaluate.** [`runtime::eval`] walks the value form, with an
//!    [`Env`] threaded through: each call takes an env *in* and returns a
//!    (possibly extended) env *out*. Special forms (`let`, `fn`, `if`, `do`,
//!    `quote`, `quasi`, `eval`, `open`, `defmacro`) are dispatched by head
//!    keyword; everything else is a function application. See the
//!    [`runtime`] module for the full evaluator and the language spec for
//!    the per-form semantics.
//!
//! `parse_and_run` is exactly: parse → fold `eval_forms` over the parser
//! output → return last value + final env.
//!
//! ---
//!
//! ## Working with values
//!
//! The runtime datatype is [`runtime::Value`]. It is `Clone`, `PartialEq`,
//! `Eq`, and `Hash`, so values can be compared and used as map keys.
//! Constructing a [`Value`] from Rust types is done with the
//! standard `From`/`Into` conversions:
//!
//! ```
//! use rizz::runtime::Value;
//! use std::rc::Rc;
//!
//! let i: Value = 42i64.into();
//! let f: Value = 3.14f64.into();
//! let s: Value = "hi".into();
//! let b: Value = true.into();             // booleans encode as Int(1) / Int(0)
//! let xs: Value = vec![1i64, 2, 3].into(); // produces a Value::Array
//! let none: Value = Option::<i64>::None.into(); // Value::Unit
//!
//! assert_eq!(i, Value::Int(42));
//! assert_eq!(b, Value::Int(1));
//! ```
//!
//! Inspecting a value:
//!
//! - [`Value::type_name`](runtime::Value::type_name) returns the variant name
//!   (`"int"`, `"str"`, `"cons"`, …) used in error messages and reflected by
//!   `(typeof v)`.
//! - [`Value::is_truthy`](runtime::Value::is_truthy) implements the language's
//!   truthiness rule. The following are false: `Unit`, `0`, `0.0`, `""`, the
//!   empty identifier, `[]`, `{}`, and any [`Value::Ref`]
//!   whose contents are falsy. Everything else — including all closures —
//!   is true.
//! - [`Value::display`](runtime::Value::display) /
//!   [`Value::repr`](runtime::Value::repr) format a value for printing.
//!   `display` is what `(to-str v)` uses; `repr` quotes strings so nested
//!   collections stay readable.
//! - `Value::iter(&Rc<Value>)` walks a cons list. A non-cons yields itself
//!   once — handy for builtins that accept "scalar or list".
//!
//! Collections use persistent `im` containers ([`im::Vector`] for arrays,
//! [`im::HashMap`] for maps), so cloning a value is cheap and structural
//! sharing is preserved when builtins return modified copies.
//!
//! ---
//!
//! ## Embedding: installing custom builtins
//!
//! The way to expose Rust functions to rizz code is to add
//! [`NativeFn`](runtime::NativeFn)s to an [`Env`] and feed that env to
//! [`Runtime::with_env`]. The pinned base env reaches every `(open ...)`d
//! module, so your builtins are visible to user modules too.
//!
//! ```
//! use rizz::{Env, Runtime, runtime::{NativeFn, Value, RuntimeError}};
//! use std::rc::Rc;
//!
//! // A pure Rust function: takes evaluated args, returns a Value.
//! let greet = NativeFn::pure("greet".into(), 1, |args| {
//!     match args[0].as_str() {
//!         Some(name) => Ok(Rc::new(Value::Str(format!("hi, {name}!").into()))),
//!         None => Err(RuntimeError::type_mismatch("greet", "str", &args[0])),
//!     }
//! })
//! .with_doc("(greet NAME)\n\nReturns a greeting string for NAME.".into());
//!
//! // Merge into the standard prelude.
//! let env = rizz::prelude::install(
//!     Env::new().update("greet".into(), Rc::new(Value::NativeFn(Rc::new(greet)))),
//! );
//!
//! let mut rt = Runtime::with_env(env);
//! let v = rt.eval(br#"(greet "world")"#.as_ref()).unwrap();
//! assert_eq!(v.as_str().as_deref(), Some("hi, world!"));
//! ```
//!
//! See the [`runtime::NativeFn`] module docs for the four flavors (`Pure`,
//! `WithEnv`, `Impure`, `Macro`) and how to choose between them. The short
//! version:
//!
//! - **`Pure`** — operates only on its evaluated arguments. Use for
//!   `+`-style primitives.
//! - **`WithEnv`** — reads the env (typically to invoke a callable argument
//!   via [`runtime::apply`]) but cannot extend it. Use for higher-order
//!   functions like `fmap`, `filter`, `reduce`.
//! - **`Impure`** — may return an extended env that is threaded back into the
//!   caller. Use for loader-style primitives that introduce bindings.
//! - **`Macro`** — receives **unevaluated** argument forms. Use for control
//!   structures.
//!
//! ---
//!
//! ## Parsing without evaluating
//!
//! Drive [`Parser`] directly when you want the AST but not the runtime — for
//! tooling, source rewriting, or feeding into your own evaluator:
//!
//! ```
//! use rizz::Parser;
//!
//! let mut p = Parser::new(b"(+ 1 2)".as_ref());
//! let forms = p.parse().unwrap();
//! assert_eq!(forms.len(), 1);
//!
//! // Every [`ParseError`] carries a [`parser::Position`].
//! let err = Parser::new(b"(1 2".as_ref()).parse().unwrap_err();
//! eprintln!("parse failed: {err}");
//! ```
//!
//! Empty (or comment-only) input is a [`ParseError`].
//!
//! ---
//!
//! ## Errors
//!
//! Failures from the library come in two families, both wrapped by
//! [`RizzError`]:
//!
//! - [`ParseError`] — surface-syntax problems (unbalanced parens, malformed
//!   numbers, non-UTF-8 bytes, unexpected end of input). Every variant
//!   carries a [`parser::Position`] so you can underline the offending byte.
//! - [`RuntimeError`] — evaluation problems (`UnknownIdent`, `NotCallable`,
//!   `ArityMismatch`, `TypeMismatch`, `ArithmeticError`, `RecursionLimit`,
//!   plus `IOError` from `open` and `InModule` wrapping any failure inside
//!   an `(open ...)`d module).
//!
//! Runaway recursion in user scripts raises
//! [`RuntimeError::RecursionLimit`](runtime::RuntimeError::RecursionLimit)
//! instead of overflowing the host stack; tune the per-thread cap with
//! [`runtime::set_recursion_limit`].
//!
//! Both implement [`std::error::Error`] via `thiserror`, so they compose with
//! `anyhow::Result` or any other error-aggregation strategy without extra
//! work.
//!
//! ---
//!
//! ## Cargo features
//!
//! - **`cli`** (off by default) — pulls in `clap` and `rustyline` and
//!   exposes the `rizz` binary plus the `cli` module. Library consumers
//!   who only want the embeddable interpreter do not need this feature.
//!
//! Add this crate as a plain library:
//!
//! ```toml
//! [dependencies]
//! rizz = "0.7"
//! ```
//!
//! Or with the CLI:
//!
//! ```toml
//! [dependencies]
//! rizz = { version = "0.7", features = ["cli"] }
//! ```
//!
//! ---
//!
//! ## Re-exports
//!
//! For convenience the most commonly needed types are re-exported at the
//! crate root: [`Parser`], [`ParseError`], [`Env`], [`Runtime`], and
//! [`RuntimeError`]. The full type universe lives under [`parser`],
//! [`runtime`], and [`prelude`].

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
mod repl;

use crate::parser::Sexp;
use crate::runtime::Value;
use std::{io::Read, rc::Rc};

pub mod consts;
pub mod parser;
pub use parser::{ParseError, Parser};
pub mod prelude;
pub mod runtime;
pub use runtime::{Env, Runtime, RuntimeError};

/// Parse every top-level form from `r` and evaluate them in source order
/// against a fresh [`Runtime`] (i.e. the default [`prelude`] env).
///
/// Forms are implicitly sequenced: each form's resulting env feeds the next,
/// so later forms see earlier `let`/`fn` bindings. Returns the value of the
/// **last** form together with the final environment.
///
/// Use this when you want a single, throwaway evaluation — a script-style run
/// or a one-off expression. For repeated calls that should share state, build
/// a [`Runtime`] and call [`Runtime::eval`] instead.
///
/// # Errors
///
/// Returns [`RizzError::ParseError`] for any surface-syntax problem, or
/// [`RizzError::RuntimeError`] if a form fails to evaluate. Empty (or
/// comment-only) input is a parse error.
///
/// # Examples
///
/// ```
/// use rizz::runtime::Value;
///
/// let (v, _env) = rizz::parse_and_run(b"(+ 1 2)".as_ref()).unwrap();
/// assert_eq!(*v, Value::Int(3));
///
/// // Forms thread bindings.
/// let (v, _env) = rizz::parse_and_run(b"(let x 4) (* x x)".as_ref()).unwrap();
/// assert_eq!(*v, Value::Int(16));
/// ```
pub fn parse_and_run<R: Read>(r: R) -> Result<(Rc<Value>, Env), RizzError> {
    let forms = parser::Parser::new(r).parse()?;
    Ok(eval_forms(forms, Runtime::new().env())?)
}

/// Like [`parse_and_run`], but evaluates against the caller-supplied `env`
/// rather than a fresh prelude.
///
/// Use this when an outer system already owns the env — for example a REPL
/// loop that accumulates bindings across user inputs and wants to evaluate
/// the next input against the running state. The returned env reflects any
/// new bindings introduced by the forms and should typically be threaded back
/// into subsequent calls.
///
/// If `env` does not include the prelude, builtins like `+` and `cond` won't
/// resolve. See [`prelude::env`] to construct a fresh prelude, or
/// [`prelude::install`] to merge one with extra bindings.
///
/// # Examples
///
/// ```
/// use rizz::runtime::Value;
///
/// let env = rizz::prelude::env();
/// let (v1, env) = rizz::parse_and_run_with_env(b"(let n 7)".as_ref(), &env).unwrap();
/// assert_eq!(*v1, Value::Int(7));
///
/// // Subsequent calls see `n` because we passed the returned env back in.
/// let (v2, _env) = rizz::parse_and_run_with_env(b"(* n 6)".as_ref(), &env).unwrap();
/// assert_eq!(*v2, Value::Int(42));
/// ```
pub fn parse_and_run_with_env<R: Read>(r: R, env: &Env) -> Result<(Rc<Value>, Env), RizzError> {
    let forms = parser::Parser::new(r).parse()?;
    Ok(eval_forms(forms, env)?)
}

/// Evaluate already-parsed `forms` in order, threading `env` between them,
/// and return the final form's value alongside the resulting env.
///
/// This is the loop that [`parse_and_run`] and [`Runtime::eval`] sit on top
/// of; reach for it when you already have a `Vec<Sexp>` (for instance from
/// driving [`Parser`] yourself or constructing forms programmatically).
///
/// `Parser::parse` rejects empty input, so a `forms` value obtained from the
/// parser is always non-empty. Calling `eval_forms` with an empty `forms`
/// returns `Value::Unit` and an unchanged env.
///
/// # Examples
///
/// ```
/// use rizz::{Parser, eval_forms, runtime::Value};
///
/// let forms = Parser::new(b"(let a 3) (let b 4) (+ a b)".as_ref()).parse().unwrap();
/// let env = rizz::prelude::env();
/// let (v, _env) = eval_forms(forms, &env).unwrap();
/// assert_eq!(*v, Value::Int(7));
/// ```
pub fn eval_forms(forms: Vec<Sexp>, env: &Env) -> Result<(Rc<Value>, Env), runtime::RuntimeError> {
    let mut env = env.clone();
    let mut last = Rc::new(Value::Unit);
    for form in forms {
        let value: Value = form.into();
        let (v, e) = runtime::eval(Rc::new(value), &env)?;
        last = v;
        env = e;
    }
    Ok((last, env))
}

/// Any failure from running a rizz program: a [`parser::ParseError`] from
/// reading the source, or a [`runtime::RuntimeError`] from evaluating it.
///
/// Both variants are `#[from]`-tagged so the `?` operator threads the
/// underlying error through transparently. Display delegates to the inner
/// error's `Display`, so you can print a `RizzError` directly without
/// matching the variant.
///
/// ```
/// match rizz::parse_and_run(b"(+ 1)".as_ref()) {
///     Ok((value, _env)) => println!("{value}"),
///     Err(e) => eprintln!("rizz failed: {e}"),
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum RizzError {
    #[error(transparent)]
    ParseError(#[from] parser::ParseError),

    #[error(transparent)]
    RuntimeError(#[from] runtime::RuntimeError),
}
