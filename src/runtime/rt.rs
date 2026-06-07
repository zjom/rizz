use std::{
    io::Read,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{
    ParseError, Parser, RizzError,
    runtime::{Env, RuntimeError, Value, eval},
};

/// Top-level evaluation handle.
///
/// Owns the persistent [`Env`] that grows as top-level forms add bindings, and
/// pins the env from which `(open ...)` loads new files. Hosts construct one
/// `Runtime` per session and call [`Runtime::eval`] / [`Runtime::eval_file`] /
/// [`Runtime::eval_form`] to drive evaluation; bindings introduced by each
/// call persist into the next.
///
/// The pinned base env is a snapshot of the env passed at construction time:
/// builtins (or any host customization) reach `open`ed modules, but top-level
/// user definitions made via this handle do not — matching the rule that
/// `open` loads against a clean module-level scope.
#[derive(Debug, Clone)]
pub struct Runtime {
    env: Env,
}

impl Runtime {
    /// Fresh runtime seeded with the default [`crate::prelude`] env.
    pub fn new() -> Self {
        Self::with_env(crate::prelude::env())
    }

    /// Runtime seeded with `env`. A snapshot of `env` is also pinned as the
    /// base for `(open ...)`, so host-installed builtins are visible to every
    /// loaded module.
    pub fn with_env(env: Env) -> Self {
        let base = Rc::new(env.clone());
        Self {
            env: env.with_base_env(base),
        }
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Evaluate a single already-parsed `form` against the current top-level
    /// env, threading the resulting env back in so later evaluations see any
    /// new bindings.
    pub fn eval_form(&mut self, form: Rc<Value>) -> Result<Rc<Value>, RuntimeError> {
        let (v, env) = eval(form, &self.env)?;
        self.env = env;
        Ok(v)
    }

    /// Parse every top-level form from `r` and evaluate them in source order,
    /// returning the value of the last form. Bindings introduced by each form
    /// are visible to the next within this call and to subsequent calls on the
    /// same runtime.
    pub fn eval<R: Read>(&mut self, r: R) -> Result<Rc<Value>, RizzError> {
        let forms = Parser::new(r).parse()?;
        let mut last: Rc<Value> = Rc::new(Value::Unit);
        for form in forms {
            let value: Value = form.into();
            last = self.eval_form(Rc::new(value))?;
        }
        Ok(last)
    }

    /// Parse and evaluate the file at `path`, setting the env's `base_dir` to
    /// the file's parent so relative `(open ...)` inside the file — and in any
    /// subsequent calls on this runtime — resolves against it.
    pub fn eval_file<P: AsRef<Path>>(&mut self, path: P) -> Result<Rc<Value>, RizzError> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|e| ParseError::from_io_error(e, None))?;
        let env = std::mem::take(&mut self.env);
        self.env = env.with_base_dir(path.parent().map(PathBuf::from));
        self.eval(file)
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
