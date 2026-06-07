use im::HashMap;
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::runtime::{NativeFn, Value};

type Inner = HashMap<Rc<str>, Rc<Value>>;

/// A lexical environment: name → value bindings, plus the contextual data
/// `(open ...)` needs to resolve and seed loaded modules.
///
/// `Env` is the unit of scope in rizz. The evaluator threads one through
/// every step — taking an env in and returning a (possibly extended) env
/// out — so top-level `let`/`fn` bindings become visible to later top-level
/// forms, and calls cleanly snap back to the caller's env when they return.
///
/// # Structure
///
/// - **`bindings`** — name → value, backed by [`im::HashMap`] so that
///   `clone` is `O(1)` and updates are persistent (structural sharing
///   between the old and new map).
/// - **`base_dir`** — anchor for relative paths in `(open "...")`. Set by
///   [`Runtime::eval_file`] to the file's parent and re-anchored on every
///   `open` so a loaded module's `(open "sibling")` resolves against its
///   own directory.
/// - **`base_env`** — the env to seed every `(open ...)`d module with.
///   Pinned by [`Runtime::with_env`] to a snapshot of the host-provided
///   env so builtins reach loaded modules; top-level user definitions made
///   *after* runtime construction do **not** propagate into modules.
///
/// # Construction
///
/// Most callers do not build an `Env` by hand — [`crate::prelude::env`]
/// returns one seeded with all the builtins, and [`crate::Runtime::new`]
/// wraps it. Construct one directly only when you want a hand-rolled
/// builtin set; [`Env::of_builtins`] is the quick path:
///
/// ```
/// use rizz::{Env, runtime::{NativeFn, Value}};
/// use std::rc::Rc;
///
/// let env = Env::of_builtins(vec![
///     ("answer", NativeFn::pure("answer".into(), 0, |_| Ok(Rc::new(Value::Int(42))))),
/// ]);
/// assert!(env.get(&Rc::from("answer")).is_some());
/// ```
///
/// # Persistence
///
/// All mutators (`update`, `union`, `filter`, `with_base_dir`,
/// `with_base_env`) consume `self` and return a new `Env`. Underneath, the
/// binding map shares structure with the input — building up a long chain
/// of `let`s does not copy the whole map each step.
///
/// [`Runtime`]: crate::runtime::Runtime
/// [`Runtime::eval_file`]: crate::runtime::Runtime::eval_file
/// [`Runtime::with_env`]: crate::runtime::Runtime::with_env
#[derive(Debug, Clone, PartialEq)]
pub struct Env {
    bindings: Inner,
    base_dir: Option<Rc<Path>>,
    base_env: Option<Rc<Env>>,
}

impl Env {
    /// An empty env: no bindings, no `base_dir`, no pinned `base_env`.
    ///
    /// An evaluator run against this env has nothing — not even arithmetic
    /// — so for almost every real use, start with [`crate::prelude::env`]
    /// or [`Runtime::new`](crate::Runtime::new) instead.
    pub fn new() -> Self {
        Self {
            bindings: Inner::new(),
            base_dir: None,
            base_env: None,
        }
    }

    /// Build an env from a list of name/`NativeFn` pairs. Each fn is
    /// wrapped in a [`Value::NativeFn`] and bound under its name.
    /// Convenience for prelude modules and tests.
    pub fn of_builtins(vals: Vec<(&str, NativeFn)>) -> Self {
        vals.into_iter().fold(Self::new(), |acc, (k, v)| {
            acc.update(k.into(), Rc::new(Value::NativeFn(Rc::new(v))))
        })
    }

    /// A new env with `k` bound to `v`. Existing binding for `k` is replaced;
    /// `base_dir` and `base_env` are preserved. Cheap — the binding map
    /// shares structure with `self`.
    pub fn update(self, k: Rc<str>, v: Rc<Value>) -> Self {
        Self {
            bindings: self.bindings.update(k, v),
            base_dir: self.base_dir,
            base_env: self.base_env,
        }
    }

    /// Look up a binding by name. Returns `None` if unbound — the same
    /// lookup the evaluator does when resolving an `Ident`, which surfaces
    /// as a [`RuntimeError::UnknownIdent`](crate::RuntimeError::UnknownIdent)
    /// in source code.
    pub fn get(&self, k: &Rc<str>) -> Option<&Rc<Value>> {
        self.bindings.get(k)
    }

    /// Union of two envs, keeping `self`'s value on key collisions.
    /// `base_dir` and `base_env` from `self` are preserved.
    ///
    /// This is the operation `(open ...)` uses to merge a loaded module's
    /// bindings into the caller — the caller's source-file context
    /// (`base_dir`) must outlive the call, and the caller's existing
    /// bindings shadow the module's.
    pub fn union(self, other: Self) -> Self {
        Self {
            bindings: self.bindings.union(other.bindings),
            base_dir: self.base_dir,
            base_env: self.base_env,
        }
    }

    /// Drop bindings for which `p` returns `false`. Used by `(open ...)`
    /// to filter out module-private (`_`-prefixed) names before merging
    /// the loaded module into the caller's env.
    pub fn filter<P>(self, p: P) -> Self
    where
        P: FnMut(&(Rc<str>, Rc<Value>)) -> bool,
    {
        Self {
            bindings: self.bindings.into_iter().filter(p).collect(),
            base_dir: self.base_dir,
            base_env: self.base_env,
        }
    }

    /// Replace the directory used to resolve relative paths in `(open ...)`.
    /// `None` falls back to the process CWD. Set by [`Runtime::eval_file`]
    /// on the runtime's env, and re-set on every `open` to the opened
    /// file's parent so nested `open`s portably resolve relative to the
    /// file doing the opening.
    ///
    /// [`Runtime::eval_file`]: crate::runtime::Runtime::eval_file
    pub fn with_base_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.base_dir = dir.map(Rc::from);
        self
    }

    /// Pin the env that `(open ...)` seeds new modules with. Typically a
    /// snapshot of the host-installed builtins — see
    /// [`Runtime::with_env`](crate::runtime::Runtime::with_env).
    pub fn with_base_env(mut self, env: Rc<Env>) -> Self {
        self.base_env = Some(env);
        self
    }

    /// The currently set base directory for relative-path resolution, or
    /// `None` if `(open ...)` should fall back to the process CWD.
    pub fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    /// The pinned base env that `(open ...)` will seed modules with, or
    /// `None` if no env is pinned (in which case modules start empty —
    /// no prelude).
    pub fn base_env(&self) -> Option<&Rc<Env>> {
        self.base_env.as_ref()
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
