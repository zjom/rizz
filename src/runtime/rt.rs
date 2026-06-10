use std::{
    io::Read,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{
    Parser, RizzError,
    runtime::{Env, RuntimeError, Value, eval},
};

/// Top-level evaluation handle. The recommended entry point for embedders
/// and REPLs.
///
/// `Runtime` owns the persistent [`Env`] that grows as top-level forms add
/// bindings, and pins the env from which `(open ...)` seeds loaded modules.
/// One `Runtime` typically corresponds to one "session" — a REPL, a single
/// script execution, a host-driven evaluation context — and bindings
/// introduced by each call persist into the next.
///
/// The pinned base env is a snapshot of the env passed at construction
/// time: builtins (or any host customization) reach `open`ed modules, but
/// **top-level user definitions made through this handle do not**, matching
/// the rule that `open` loads against a clean module-level scope.
///
/// # Examples
///
/// A short REPL-style session:
///
/// ```
/// use rizz::{Runtime, runtime::Value};
///
/// let mut rt = Runtime::new();
/// rt.eval(b"(let counter (ref 0))".as_ref()).unwrap();
/// rt.eval(b"(set! counter (+ (deref counter) 1))".as_ref()).unwrap();
/// rt.eval(b"(set! counter (+ (deref counter) 1))".as_ref()).unwrap();
/// let v = rt.eval(b"(deref counter)".as_ref()).unwrap();
/// assert_eq!(*v, Value::Int(2));
/// ```
#[derive(Debug, Clone)]
pub struct Runtime {
    env: Env,
}

impl Runtime {
    /// A fresh runtime seeded with the default [`crate::prelude`] env.
    /// The prelude is also pinned as the base env for `(open ...)`.
    pub fn new() -> Self {
        Self::with_env(crate::prelude::env())
    }

    /// A runtime seeded with `env`. A snapshot of `env` is pinned as the
    /// base for `(open ...)`, so host-installed builtins are visible to
    /// every loaded module.
    ///
    /// Use this to inject custom builtins:
    ///
    /// ```
    /// use rizz::{Env, Runtime, runtime::{NativeFn, Value}};
    /// use std::rc::Rc;
    ///
    /// let f = NativeFn::pure("six".into(), 0, |_| Ok(Rc::new(Value::Int(6))));
    /// let env = rizz::prelude::install(
    ///     Env::new().update("six".into(), Rc::new(Value::NativeFn(Rc::new(f)))),
    /// );
    /// let mut rt = Runtime::with_env(env);
    /// assert_eq!(*rt.eval(b"(* (six) 7)".as_ref()).unwrap(), Value::Int(42));
    /// ```
    pub fn with_env(env: Env) -> Self {
        let base = Rc::new(env.clone());
        Self {
            env: env.with_base_env(base),
        }
    }

    /// The current top-level env. Useful for inspecting bindings after a
    /// run, or for handing the env to [`crate::parse_and_run_with_env`].
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Evaluate a single already-parsed `form` against the current
    /// top-level env, threading the resulting env back in so later calls
    /// see any new bindings.
    ///
    /// Use this to feed pre-parsed forms one at a time (e.g. when forms
    /// come from a custom front-end). For source text use [`Runtime::eval`].
    pub fn eval_form(&mut self, form: Rc<Value>) -> Result<Rc<Value>, RuntimeError> {
        let (v, env) = eval(form, &self.env)?;
        self.env = env;
        Ok(v)
    }

    /// Parse every top-level form from `r` and evaluate them in source
    /// order, returning the value of the last form.
    ///
    /// Bindings introduced by each form are visible to the next within
    /// this call **and** to subsequent calls on the same runtime — this is
    /// what makes incremental input (REPL-style) work.
    pub fn eval<R: Read>(&mut self, r: R) -> Result<Rc<Value>, RizzError> {
        let forms = Parser::new(r).parse()?;
        let mut last: Rc<Value> = Rc::new(Value::Unit);
        for form in forms {
            let value: Value = form.into();
            last = self.eval_form(Rc::new(value))?;
        }
        Ok(last)
    }

    /// Parse and evaluate the file at `path`, anchoring the env's
    /// `base_dir` to the file's parent so that relative `(open ...)`s
    /// inside the file — and on any subsequent calls on this runtime —
    /// resolve against it.
    ///
    /// I/O failures from opening the file are surfaced as
    /// [`RuntimeError::IOError`] — the same family `(open ...)` uses for a
    /// missing module file.
    pub fn eval_file<P: AsRef<Path>>(&mut self, path: P) -> Result<Rc<Value>, RizzError> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(RuntimeError::IOError)?;
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
