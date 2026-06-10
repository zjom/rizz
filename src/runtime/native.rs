//! Host-language (Rust) functions exposed to the rizz runtime.
//!
//! A [`NativeFn`] is a callable implemented in Rust and reachable from rizz
//! source as a [`Value::NativeFn`]. The bulk of the standard library
//! (`src/prelude/*`) is built out of these — see e.g. [`crate::prelude`].
//!
//! # Why four variants?
//!
//! The evaluator threads an [`Env`] through evaluation: each step takes an env
//! in and returns an env out. Native functions sit at the boundary between
//! the interpreter and Rust code, and they vary along two axes:
//!
//! - **Do they need the env at all?** Most arithmetic/string/collection
//!   primitives (e.g. `+`, `len`, `car`) operate purely on their arguments
//!   and want nothing to do with `Env`.
//! - **Should they be able to *change* the env?** A function that just maps
//!   a callback over a collection (`fmap`, `filter`, `reduce`) needs to
//!   *read* the env in order to look up and apply the callback, but the
//!   bindings introduced while running it must not leak into the caller's
//!   scope. Conversely, a few primitives genuinely *do* extend the env (none
//!   ship today, but the variant exists so module/loader-style builtins can
//!   be added without going through the special-form machinery).
//!
//! Encoding these distinctions at the type level — instead of giving every
//! function the maximally-powerful `Fn(&[Value], &Env) -> (Value, Env)` shape
//! and trusting authors to return `env.clone()` when they don't mean to
//! mutate — makes the contract explicit and makes accidental env leaks
//! impossible for the common cases.
//!
//! | Variant     | Sees `env`? | Returned env propagates? | Args pre-evaluated? |
//! |-------------|:-----------:|:------------------------:|:-------------------:|
//! | [`Pure`]    |      no     |          n/a             |        yes          |
//! | [`WithEnv`] |     yes     |          no              |        yes          |
//! | [`Impure`]  |     yes     |          yes             |        yes          |
//! | [`Macro`]   |     yes     |          no              |         no          |
//!
//! [`Pure`]: NativeFn::Pure
//! [`WithEnv`]: NativeFn::WithEnv
//! [`Impure`]: NativeFn::Impure
//! [`Macro`]: NativeFn::Macro
//!
//! # Choosing a variant
//!
//! Walk down this list and pick the first row that fits — variants further
//! down grant strictly more power, and giving a function more power than it
//! needs costs clarity and removes a static guarantee against env leaks.
//!
//! 1. **Operates only on its arguments?** Use [`pure`](NativeFn::pure).
//!    Examples: `+`, `len`, `car`, `typeof`.
//! 2. **Needs to invoke a callable passed as an argument** (so it must call
//!    back into the evaluator via [`crate::runtime::apply`])? Use
//!    [`with_env`](NativeFn::with_env). The env is available for read but the
//!    function returns only a value; any bindings the callback introduces
//!    stay scoped to the call. Examples: `fmap`, `filter`, `reduce`, `show`.
//! 3. **Genuinely extends the caller's env** (defines new bindings that
//!    should outlive the call)? Use [`impure`](NativeFn::impure). The env it
//!    returns is threaded back into the caller. The `open` special form does
//!    something like this but is currently implemented as a special form
//!    rather than a native fn — this variant exists for future
//!    loader/import-style primitives.
//! 4. **Wants its arguments unevaluated** (i.e. it's a macro implemented in
//!    Rust)? Use [`macro_`](NativeFn::macro_). The body receives the raw
//!    argument forms and is responsible for producing the result. Like
//!    [`WithEnv`](NativeFn::WithEnv), env access is read-only.
//!
//! # Invocation paths
//!
//! Two entry points cover all uses:
//!
//! - [`NativeFn::call`] — used by the evaluator at a source-level call site
//!   `(f arg1 arg2 ...)`. Receives the *unevaluated* arg list (`tail`) and
//!   evaluates it for all variants except `Macro`, which by definition wants
//!   the raw forms.
//! - [`NativeFn::apply`] — used by higher-order builtins (themselves
//!   typically [`WithEnv`](NativeFn::WithEnv)) via the public
//!   [`crate::runtime::apply`] helper. Receives *already-evaluated* args,
//!   skipping the eval-and-collect step. Macros cannot be applied this way:
//!   their whole purpose is to receive unevaluated forms, so calling
//!   `apply` on a macro is a type error.
//!
//! Both methods return `(Rc<Value>, Env)`; what populates the `Env` slot
//! depends on the variant (see the table above).
//!
//! # Arity
//!
//! Each variant carries an `nargs` field. Checking is **lower-bound only**:
//! the runtime guarantees the function receives *at least* `nargs`
//! arguments, but allows more. Passing `nargs = 0` opts out of checking
//! entirely and is the idiom for variadic functions. Functions that want a
//! strict upper bound must check `args.len()` themselves and return
//! [`RuntimeError::ArityMismatch`].
//!
//! # Names and docs
//!
//! Every variant carries a `name` (used in error messages) and an optional
//! `doc` slot (surfaced by the `show` builtin). The `doc` slot starts empty
//! and is populated via [`NativeFn::with_doc`], typically chained onto a
//! constructor call:
//!
//! ```ignore
//! NativeFn::pure("len".into(), 1, |args| { /* ... */ })
//!     .with_doc("(len COLL)\n\nReturns int: the element count of COLL.".into())
//! ```

use std::rc::Rc;

use crate::{
    Env, Value,
    runtime::{self, RuntimeError},
};

/// A native function that does not see the [`Env`]. See [`NativeFn::Pure`].
pub type PureFn = Rc<dyn Fn(&[Rc<Value>]) -> Result<Rc<Value>, RuntimeError>>;

/// A native function that reads the [`Env`] (e.g. to invoke a callable arg
/// via [`crate::runtime::apply`]) but cannot extend it. Returned value is
/// the call's result; the caller's env is preserved by [`NativeFn::call`].
/// See [`NativeFn::WithEnv`] and [`NativeFn::Macro`].
pub type WithEnvFn = Rc<dyn Fn(&[Rc<Value>], &Env) -> Result<Rc<Value>, RuntimeError>>;

/// A native function that reads the [`Env`] and may extend it. The returned
/// env is threaded out of the call site by the evaluator. See
/// [`NativeFn::Impure`].
pub type ImpureFn = Rc<dyn Fn(&[Rc<Value>], &Env) -> Result<(Rc<Value>, Env), RuntimeError>>;

/// A callable implemented in Rust. See the module docs for an overview and
/// guidance on choosing a variant.
#[derive(Clone)]
pub enum NativeFn {
    /// Stateless: takes only the evaluated arguments and returns a value.
    /// Most arithmetic and pure-data builtins live here. Constructed via
    /// [`NativeFn::pure`].
    Pure {
        f: PureFn,
        nargs: usize,
        name: Rc<str>,
        doc: Option<Rc<str>>,
    },

    /// Reads the env (typically to invoke a higher-order callable argument
    /// via [`crate::runtime::apply`]) but cannot mutate it: any bindings
    /// introduced during the call are dropped at the call boundary.
    /// Constructed via [`NativeFn::with_env`].
    ///
    /// This is the right variant for functions like `fmap`, `filter`,
    /// `reduce`, and `show`.
    WithEnv {
        f: WithEnvFn,
        nargs: usize,
        name: Rc<str>,
        doc: Option<Rc<str>>,
    },

    /// A Rust-implemented macro: receives its arguments **unevaluated** and
    /// is responsible for producing the resulting value (which is *not*
    /// re-evaluated in the caller's env — this differs from a user-defined
    /// [`Value::Macro`] whose body's result is evaluated). Env is read-only,
    /// matching [`WithEnv`](Self::WithEnv). Constructed via
    /// [`NativeFn::macro_`]. Cannot be invoked through
    /// [`apply`](Self::apply), since apply operates on evaluated values.
    Macro {
        f: WithEnvFn,
        nargs: usize,
        name: Rc<str>,
        doc: Option<Rc<str>>,
    },

    /// Genuinely env-extending: the env returned by `f` is propagated back
    /// out of the call site so the function can introduce bindings visible
    /// to subsequent forms in the caller's scope. Constructed via
    /// [`NativeFn::impure`].
    ///
    /// No prelude builtin ships in this variant today; it exists for
    /// loader/import-style primitives that need to splice bindings into the
    /// surrounding env without going through a special form.
    Impure {
        f: ImpureFn,
        nargs: usize,
        name: Rc<str>,
        doc: Option<Rc<str>>,
    },
}

impl NativeFn {
    /// Builds a [`Pure`](Self::Pure) function: no env access, args only.
    ///
    /// `name` appears in error messages. `nargs` is the minimum arity (`0`
    /// disables checking — use for variadic functions); the runtime
    /// guarantees `f` receives at least `nargs` arguments. See the
    /// module-level "Arity" section.
    ///
    /// The returned `NativeFn` carries no doc by default; chain
    /// [`with_doc`](Self::with_doc) to attach one.
    pub fn pure<F>(name: Rc<str>, nargs: usize, f: F) -> NativeFn
    where
        F: Fn(&[Rc<Value>]) -> Result<Rc<Value>, RuntimeError> + 'static,
    {
        NativeFn::Pure {
            f: Rc::new(f),
            nargs,
            name,
            doc: None,
        }
    }

    /// Builds a [`WithEnv`](Self::WithEnv) function: reads `env`, returns
    /// a value only.
    ///
    /// Use this for any function that needs to invoke a callable passed in
    /// as an argument (typically via [`crate::runtime::apply`]) — the env
    /// is required to resolve identifiers the callable closes over. The
    /// returned env from [`call`](Self::call) is always the caller's
    /// original env, so bindings introduced inside the call cannot leak.
    ///
    /// `name`, `nargs`, and the no-doc default follow the same rules as
    /// [`pure`](Self::pure).
    pub fn with_env<F>(name: Rc<str>, nargs: usize, f: F) -> NativeFn
    where
        F: Fn(&[Rc<Value>], &Env) -> Result<Rc<Value>, RuntimeError> + 'static,
    {
        NativeFn::WithEnv {
            f: Rc::new(f),
            nargs,
            name,
            doc: None,
        }
    }

    /// Builds an [`Impure`](Self::Impure) function: reads `env` and may
    /// return an extended env that the evaluator threads back into the
    /// caller's scope.
    ///
    /// Reach for this only when the function genuinely needs to introduce
    /// bindings that outlive the call. If you only need to *read* env to
    /// dispatch a callback, use [`with_env`](Self::with_env) instead — it
    /// gives a stronger static guarantee that bindings don't leak.
    ///
    /// `name`, `nargs`, and the no-doc default follow the same rules as
    /// [`pure`](Self::pure).
    pub fn impure<F>(name: Rc<str>, nargs: usize, f: F) -> NativeFn
    where
        F: Fn(&[Rc<Value>], &Env) -> Result<(Rc<Value>, Env), RuntimeError> + 'static,
    {
        NativeFn::Impure {
            f: Rc::new(f),
            nargs,
            name,
            doc: None,
        }
    }

    /// Builds a [`Macro`](Self::Macro) function: receives arguments
    /// **unevaluated** and produces a value directly (not re-evaluated by
    /// the caller — unlike user-defined macros).
    ///
    /// `name`, `nargs`, and the no-doc default follow the same rules as
    /// [`pure`](Self::pure). Note that `nargs` is still checked against the
    /// raw argument count.
    pub fn macro_<F>(name: Rc<str>, nargs: usize, f: F) -> NativeFn
    where
        F: Fn(&[Rc<Value>], &Env) -> Result<Rc<Value>, RuntimeError> + 'static,
    {
        NativeFn::Macro {
            f: Rc::new(f),
            nargs,
            name,
            doc: None,
        }
    }

    /// Returns this fn with its `doc` slot replaced. Typically chained onto
    /// a constructor call — see the module-level "Names and docs" section.
    pub fn with_doc(mut self, doc: Rc<str>) -> Self {
        match &mut self {
            Self::Pure { doc: d, .. }
            | Self::WithEnv { doc: d, .. }
            | Self::Macro { doc: d, .. }
            | Self::Impure { doc: d, .. } => *d = Some(doc),
        }
        self
    }

    /// The doc string attached at definition, or `None` if none was set.
    /// Surfaced by the `show` builtin (see [`crate::prelude::meta`]).
    /// Constructors leave this empty; use [`with_doc`](Self::with_doc) to
    /// populate it.
    pub fn doc(&self) -> Option<Rc<str>> {
        match self {
            Self::Pure { doc, .. }
            | Self::WithEnv { doc, .. }
            | Self::Macro { doc, .. }
            | Self::Impure { doc, .. } => doc.clone(),
        }
    }

    /// Invokes this fn at a source-level call site `(self . tail)`.
    ///
    /// `tail` is the unevaluated argument list (a cons-list `Value`). For
    /// every variant except [`Macro`](Self::Macro), the args are evaluated
    /// left-to-right via [`runtime::eval_and_collect`] before being passed
    /// to `f`; the [`Macro`](Self::Macro) arm passes the raw forms through.
    ///
    /// The returned env is the env the evaluator should thread out of the
    /// call site:
    /// - [`Pure`](Self::Pure), [`WithEnv`](Self::WithEnv),
    ///   [`Macro`](Self::Macro): always the caller's `env` unchanged. Any
    ///   bindings introduced while evaluating siblings (e.g. `(plus (let
    ///   x 5) x)`) thread between args but do not escape the call.
    /// - [`Impure`](Self::Impure): the env returned by `f`. This is how
    ///   loader-style natives extend the caller's scope.
    ///
    /// Errors from arg evaluation, arity checking, or `f` itself propagate
    /// unchanged.
    pub fn call(&self, tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
        match self {
            Self::Pure { f, nargs, name, .. } => {
                let (args, _) = runtime::eval_and_collect(tail, env)?;
                validate_args(name, &args, *nargs)?;
                f(&args).map(|v| (v, env.clone()))
            }
            Self::WithEnv { f, nargs, name, .. } => {
                let (args, sibling_env) = runtime::eval_and_collect(tail, env)?;
                validate_args(name, &args, *nargs)?;
                f(&args, &sibling_env).map(|v| (v, env.clone()))
            }
            Self::Impure { f, nargs, name, .. } => {
                let (args, sibling_env) = runtime::eval_and_collect(tail, env)?;
                validate_args(name, &args, *nargs)?;
                f(&args, &sibling_env)
            }
            Self::Macro { f, nargs, name, .. } => {
                let args: Vec<_> = Value::iter(tail).collect();
                validate_args(name, &args, *nargs)?;
                f(&args, env).map(|v| (v, env.clone()))
            }
        }
    }

    /// Applies this fn to **already-evaluated** `args` — the higher-order
    /// counterpart to [`call`](Self::call). Used by builtins that receive a
    /// callable as a value (e.g. `fmap`, `filter`) and need to invoke it
    /// against runtime values, via the public [`crate::runtime::apply`]
    /// helper.
    ///
    /// Arity is checked the same way as in [`call`](Self::call). The
    /// [`Macro`](Self::Macro) arm rejects with [`RuntimeError::TypeMismatch`]
    /// because macros operate on unevaluated forms by definition — there is
    /// no sensible way to apply one to a `Vec` of values.
    ///
    /// The [`Impure`](Self::Impure) arm returns the env its function
    /// produced. The public [`crate::runtime::apply`] wrapper discards that
    /// env (which is what callers almost always want, since the higher-order
    /// builtin is itself usually [`WithEnv`](Self::WithEnv) and so cannot
    /// propagate env changes anyway). Callers that need the produced env
    /// must invoke `apply` directly on the `NativeFn` and read the second
    /// tuple element themselves.
    pub fn apply(&self, args: &[Rc<Value>], env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
        match self {
            Self::Pure { f, nargs, name, .. } => {
                validate_args(name, args, *nargs)?;
                f(args).map(|v| (v, env.clone()))
            }
            Self::WithEnv { f, nargs, name, .. } => {
                validate_args(name, args, *nargs)?;
                f(args, env).map(|v| (v, env.clone()))
            }
            Self::Impure { f, nargs, name, .. } => {
                validate_args(name, args, *nargs)?;
                f(args, env)
            }
            Self::Macro { name, .. } => Err(RuntimeError::TypeMismatch {
                name: name.clone(),
                expected: "applicable (pure/with-env/impure) fn".into(),
                got: "macro".into(),
            }),
        }
    }
}

/// Lower-bound arity check shared by [`NativeFn::call`] and
/// [`NativeFn::apply`]. `nargs == 0` opts out (the variadic convention);
/// otherwise `args.len() < nargs` is an [`RuntimeError::ArityMismatch`].
/// There is no upper-bound check — functions that want one must enforce it
/// themselves.
fn validate_args(name: &Rc<str>, args: &[Rc<Value>], nargs: usize) -> Result<(), RuntimeError> {
    if nargs == 0 {
        return Ok(());
    }
    if args.len() < nargs {
        return Err(RuntimeError::ArityMismatch {
            name: name.clone(),
            expected: crate::runtime::Arity::AtLeast(nargs),
            got: args.len(),
        });
    }
    Ok(())
}
