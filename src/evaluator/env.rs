use im::HashMap;
use std::{fmt::Debug, rc::Rc};

use crate::evaluator::{BuiltinFn, Value};

type Inner = HashMap<Rc<str>, Rc<Value>>;

/// An immutable map from names to values, backed by a persistent [`im::HashMap`]
/// so it can be cheaply cloned and shared. Cloning is how closures capture
/// their defining scope and how callers thread bindings through evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct Env(Inner);

impl Env {
    /// An empty environment with no bindings.
    pub fn new() -> Self {
        Self(Inner::new())
    }

    /// Builds an environment from `(name, builtin)` pairs, wrapping each
    /// function as a [`Value::BuiltinFn`]. Used to assemble the prelude.
    pub fn of_builtins(vals: Vec<(&str, BuiltinFn)>) -> Self {
        vals.into_iter().fold(Self::new(), |acc, (k, v)| {
            acc.update(k.into(), Rc::new(Value::BuiltinFn(v)))
        })
    }

    /// Returns a new environment with `k` bound to `v`, shadowing any existing
    /// binding. The receiver is left unchanged (the map is persistent).
    pub fn update(self, k: Rc<str>, v: Rc<Value>) -> Self {
        Self(self.0.update(k, v))
    }

    /// Looks up the value bound to `k`, if any.
    pub fn get(&self, k: &Rc<str>) -> Option<&Rc<Value>> {
        self.0.get(k)
    }

    /// Construct the union of two maps, keeping the values in the
    /// current map when keys exist in both maps.
    pub fn union(self, other: Self) -> Self {
        Self(self.0.union(other.0))
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
