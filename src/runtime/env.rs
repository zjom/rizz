use im::HashMap;
use std::{fmt::Debug, rc::Rc};

use crate::runtime::{NativeFn, Value};

type Inner = HashMap<Rc<str>, Rc<Value>>;
#[derive(Debug, Clone, PartialEq)]
pub struct Env(Inner);

impl Env {
    pub fn new() -> Self {
        Self(Inner::new())
    }

    pub fn of_builtins(vals: Vec<(&str, NativeFn)>) -> Self {
        vals.into_iter().fold(Self::new(), |acc, (k, v)| {
            acc.update(k.into(), Rc::new(Value::NativeFn(Rc::new(v))))
        })
    }

    /// Construct a new hash map by inserting a key/value mapping into a map.
    pub fn update(self, k: Rc<str>, v: Rc<Value>) -> Self {
        Self(self.0.update(k, v))
    }

    pub fn get(&self, k: &Rc<str>) -> Option<&Rc<Value>> {
        self.0.get(k)
    }

    /// Construct the union of two maps, keeping the values in the
    /// current map when keys exist in both maps.
    pub fn union(self, other: Self) -> Self {
        Self(self.0.union(other.0))
    }

    pub fn filter<P>(self, p: P) -> Self
    where
        P: FnMut(&(Rc<str>, Rc<Value>)) -> bool,
    {
        Env(self.0.into_iter().filter(p).collect())
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
