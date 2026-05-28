use im::HashMap;
use std::{fmt::Debug, rc::Rc};

use crate::evaluator::{BuiltinFn, Value};

type Inner = HashMap<Rc<str>, Rc<Value>>;
#[derive(Debug, Clone, PartialEq)]
pub struct Env(Inner);

impl Env {
    pub fn new() -> Self {
        Self(Inner::new())
    }

    pub fn of_builtins(vals: Vec<(&str, BuiltinFn)>) -> Self {
        vals.into_iter().fold(Self::new(), |acc, (k, v)| {
            acc.update(k.into(), Rc::new(Value::BuiltinFn(v)))
        })
    }

    pub fn update(self, k: Rc<str>, v: Rc<Value>) -> Self {
        Self(self.0.update(k, v))
    }

    pub fn get(&self, k: &Rc<str>) -> Option<&Rc<Value>> {
        self.0.get(k)
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0.union(other.0))
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
