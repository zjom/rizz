use im::HashMap;
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{
    prelude,
    runtime::{NativeFn, Value},
};

type Inner = HashMap<Rc<str>, Rc<Value>>;
#[derive(Debug, Clone, PartialEq)]
pub struct Env {
    bindings: Inner,
    /// Directory used to anchor relative paths in I/O builtins like `open`.
    /// `None` falls back to the process CWD.
    base_dir: Option<Rc<Path>>,
    base_env: Rc<Env>,
}

impl Env {
    pub fn new() -> Self {
        Self {
            bindings: Inner::new(),
            base_dir: None,
            base_env: Rc::new(prelude::env()),
        }
    }

    pub fn of_builtins(vals: Vec<(&str, NativeFn)>) -> Self {
        vals.into_iter().fold(Self::new(), |acc, (k, v)| {
            acc.update(k.into(), Rc::new(Value::NativeFn(Rc::new(v))))
        })
    }

    /// Construct a new hash map by inserting a key/value mapping into a map.
    pub fn update(self, k: Rc<str>, v: Rc<Value>) -> Self {
        Self {
            bindings: self.bindings.update(k, v),
            base_dir: self.base_dir,
            base_env: self.base_env,
        }
    }

    pub fn get(&self, k: &Rc<str>) -> Option<&Rc<Value>> {
        self.bindings.get(k)
    }

    /// Construct the union of two maps, keeping the values in the
    /// current map when keys exist in both maps. The current map's
    /// `base_dir` is also preserved — `union` is used by `open` to merge a
    /// loaded module's bindings into the caller's env, and the caller's
    /// source-file context should outlive the call.
    pub fn union(self, other: Self) -> Self {
        Self {
            bindings: self.bindings.union(other.bindings),
            base_dir: self.base_dir,
            base_env: self.base_env,
        }
    }

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

    pub fn with_base_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.base_dir = dir.map(Rc::from);
        self
    }

    pub fn with_base_env(mut self, env: Env) -> Self {
        self.base_env = Rc::new(env);
        self
    }

    pub fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    pub fn base_env(&self) -> &Env {
        &self.base_env
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
