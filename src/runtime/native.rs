use std::rc::Rc;

use crate::{
    Env, Value,
    runtime::{self, RuntimeError},
};

pub type PureFn = Rc<dyn Fn(&[Rc<Value>]) -> Result<Rc<Value>, RuntimeError>>;
pub type ImpureFn = Rc<dyn Fn(&[Rc<Value>], &Env) -> Result<(Rc<Value>, Env), RuntimeError>>;
#[derive(Clone)]
pub enum NativeFn {
    Pure {
        f: PureFn,
        nargs: usize,
        name: Rc<str>,
        doc: Option<Rc<str>>,
    },
    Impure {
        f: ImpureFn,
        nargs: usize,
        name: Rc<str>,
        doc: Option<Rc<str>>,
    },

    Macro {
        f: ImpureFn,
        nargs: usize,
        name: Rc<str>,
        doc: Option<Rc<str>>,
    },
}

impl NativeFn {
    /// construct a function that does care about the [`Env`]
    /// set `nargs` to 0 for a variadic function.
    /// arity checking is done on an at least basis. ie if you specify `nargs`,
    /// we guarantee that you will receive AT LEAST that many args.
    /// there may or may not be more args after.
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

    /// construct a function that does may or may not care about the [`Env`]
    /// set `nargs` to 0 for a variadic function.
    /// arity checking is done on an at least basis. ie if you specify `nargs`,
    /// we guarantee that you will receive AT LEAST that many args.
    /// there may or may not be more args after.
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
    pub fn macro_<F>(name: Rc<str>, nargs: usize, f: F) -> NativeFn
    where
        F: Fn(&[Rc<Value>], &Env) -> Result<(Rc<Value>, Env), RuntimeError> + 'static,
    {
        NativeFn::Macro {
            f: Rc::new(f),
            nargs,
            name,
            doc: None,
        }
    }

    /// Returns this fn with its `doc` slot replaced.
    pub fn with_doc(mut self, doc: Rc<str>) -> Self {
        match &mut self {
            Self::Pure { doc: d, .. }
            | Self::Impure { doc: d, .. }
            | Self::Macro { doc: d, .. } => *d = Some(doc),
        }
        self
    }

    /// The doc string attached at definition (always `None` for builtins
    /// constructed via [`pure`](Self::pure)/[`impure`](Self::impure)/[`macro_`](Self::macro_)
    /// unless [`with_doc`](Self::with_doc) was used).
    pub fn doc(&self) -> Option<Rc<str>> {
        match self {
            Self::Pure { doc, .. } | Self::Impure { doc, .. } | Self::Macro { doc, .. } => {
                doc.clone()
            }
        }
    }

    pub fn call(&self, tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
        match self {
            Self::Pure { f, nargs, name, .. } => {
                let (args, env) = runtime::eval_and_collect(tail, env)?;
                validate_args(name, &args, *nargs)?;
                f(&args).map(|v| (v, env.clone()))
            }
            Self::Impure { f, nargs, name, .. } => {
                let (args, env) = runtime::eval_and_collect(tail, env)?;
                validate_args(name, &args, *nargs)?;
                f(&args, &env)
            }
            Self::Macro { f, nargs, name, .. } => {
                let args: Vec<_> = Value::iter(tail).collect();
                validate_args(name, &args, *nargs)?;
                f(&args, env)
            }
        }
    }

    /// Applies the fn to **already-evaluated** `args` (unlike [`call`](Self::call),
    /// which evaluates an unevaluated tail). Used by higher-order builtins via
    /// [`crate::runtime::apply`]. Macros cannot be applied to values.
    ///
    /// The `Impure` arm returns the env its function produced. Callers that want
    /// env-isolation (the common case) should discard that env and keep the
    /// caller's — see [`crate::runtime::apply`].
    pub fn apply(&self, args: &[Rc<Value>], env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
        match self {
            Self::Pure { f, nargs, name, .. } => {
                validate_args(name, args, *nargs)?;
                f(args).map(|v| (v, env.clone()))
            }
            Self::Impure { f, nargs, name, .. } => {
                validate_args(name, args, *nargs)?;
                f(args, env)
            }
            Self::Macro { name, .. } => Err(RuntimeError::TypeMismatch {
                name: name.clone(),
                expected: "applicable (pure/impure) fn".into(),
                got: "macro".into(),
            }),
        }
    }
}

fn validate_args(name: &Rc<str>, args: &[Rc<Value>], nargs: usize) -> Result<(), RuntimeError> {
    if nargs == 0 {
        return Ok(());
    }
    if args.len() < nargs {
        return Err(RuntimeError::ArityMismatch {
            name: name.clone(),
            expected: nargs,
            got: args.len(),
        });
    }
    Ok(())
}
