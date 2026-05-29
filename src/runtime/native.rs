use std::rc::Rc;

use crate::{
    Env, Value,
    runtime::{self, RuntimeError},
};

pub type PureFn = Rc<dyn Fn(&[Rc<Value>]) -> Result<Rc<Value>, RuntimeError>>;
pub type ImpureFn = Rc<dyn Fn(&[Rc<Value>], &Env) -> Result<(Rc<Value>, Env), RuntimeError>>;
pub enum NativeFn {
    Pure {
        f: PureFn,
        nargs: usize,
        name: Rc<str>,
    },
    Impure {
        f: ImpureFn,
        nargs: usize,
        name: Rc<str>,
    },

    Macro {
        f: ImpureFn,
        nargs: usize,
        name: Rc<str>,
    },
}

impl NativeFn {
    pub fn pure<F>(name: Rc<str>, nargs: usize, f: F) -> NativeFn
    where
        F: Fn(&[Rc<Value>]) -> Result<Rc<Value>, RuntimeError> + 'static,
    {
        assert!(nargs > 0);
        NativeFn::Pure {
            f: Rc::new(f),
            nargs,
            name,
        }
    }

    pub fn impure<F>(name: Rc<str>, nargs: usize, f: F) -> NativeFn
    where
        F: Fn(&[Rc<Value>], &Env) -> Result<(Rc<Value>, Env), RuntimeError> + 'static,
    {
        NativeFn::Impure {
            f: Rc::new(f),
            nargs,
            name,
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
        }
    }

    /// Applies the fn to **already-evaluated** `args` (unlike [`call`], which
    /// evaluates an unevaluated tail). Used by higher-order builtins via
    /// [`crate::runtime::apply`]. Macros cannot be applied to values.
    pub fn apply(
        &self,
        args: &[Rc<Value>],
        env: &Env,
    ) -> Result<(Rc<Value>, Env), RuntimeError> {
        match self {
            Self::Pure { f, nargs, name } => {
                validate_args(name, args, *nargs)?;
                f(args).map(|v| (v, env.clone()))
            }
            Self::Impure { f, nargs, name } => {
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

    pub fn call(&self, tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
        match self {
            Self::Pure { f, nargs, name } => {
                let (args, env) = runtime::eval_and_collect(tail, env)?;
                validate_args(name, &args, *nargs)?;
                f(&args).map(|v| (v, env.clone()))
            }
            Self::Impure { f, nargs, name } => {
                let (args, env) = runtime::eval_and_collect(tail, env)?;
                validate_args(name, &args, *nargs)?;
                f(&args, &env)
            }
            Self::Macro { f, nargs, name } => {
                let args: Vec<_> = Value::iter(tail).collect();
                validate_args(name, &args, *nargs)?;
                f(&args, env)
            }
        }
    }
}

fn validate_args(name: &Rc<str>, args: &[Rc<Value>], nargs: usize) -> Result<(), RuntimeError> {
    if args.len() != nargs {
        return Err(RuntimeError::ArityMismatch {
            name: name.clone(),
            expected: nargs,
            got: args.len(),
        });
    }
    Ok(())
}
