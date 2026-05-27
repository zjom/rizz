use im::HashMap;
use std::{fmt::Debug, rc::Rc};

use crate::{
    evaluator::EvaluatorError,
    parser::{Atomic, Sexp},
};

pub type Env = HashMap<Rc<str>, Rc<Value>>;

pub type BuiltinFn = Rc<dyn Fn(&[Rc<Value>], &Env) -> Result<Rc<Value>, EvaluatorError>>;

#[derive(Clone)]
pub enum Value {
    Str(Rc<str>),
    Int(i64),
    Float(f64),
    Ident(Rc<str>),
    Unit,
    Cons {
        head: Rc<Value>,
        tail: Rc<Value>,
    },
    BuiltinFn(BuiltinFn),
    Closure {
        params: Vec<Rc<str>>,
        body: Rc<Value>,
        env: Env,
    },
}

impl Value {
    pub fn iter(value: &Rc<Value>) -> impl Iterator<Item = Rc<Value>> {
        Iter::new(value.clone())
    }

    pub fn is_callable(&self) -> bool {
        matches!(self, Value::BuiltinFn(_) | Value::Closure { .. })
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DepthLimited {
            value: self,
            depth: MAX_DEBUG_DEPTH,
        }
        .fmt(f)
    }
}

impl IntoIterator for Value {
    type Item = Rc<Value>;
    type IntoIter = Iter;
    fn into_iter(self) -> Self::IntoIter {
        Iter { cur: Rc::new(self) }
    }
}

const MAX_DEBUG_DEPTH: usize = 4;

/// Wrapper that formats a `Value` with a remaining-depth budget.
struct DepthLimited<'a> {
    value: &'a Value,
    depth: usize,
}

impl<'a> Debug for DepthLimited<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.value {
            Value::Str(s) => write!(f, "<str=\"{s}\">"),
            Value::Ident(s) => write!(f, "<ident={s}>"),
            Value::Float(n) => write!(f, "<float={n}>"),
            Value::Int(n) => write!(f, "<int={n}>"),
            Value::Unit => write!(f, "<()>"),
            Value::BuiltinFn(_) => write!(f, "<builtin_fn>"),
            Value::Closure { params, .. } => write!(f, "<closure params={:?}>", params),
            Value::Cons { head, tail } => {
                if self.depth == 0 {
                    write!(f, "<expr ...>")
                } else {
                    let next = self.depth - 1;
                    write!(
                        f,
                        "<expr head={:?} tail={:?}>",
                        DepthLimited {
                            value: head,
                            depth: next
                        },
                        DepthLimited {
                            value: tail,
                            depth: next
                        },
                    )
                }
            }
        }
    }
}

pub struct Iter {
    cur: Rc<Value>,
}
impl Iter {
    pub fn new(value: Rc<Value>) -> Iter {
        Iter { cur: value.clone() }
    }
}
impl Iterator for Iter {
    type Item = Rc<Value>;
    fn next(&mut self) -> Option<Self::Item> {
        match &*self.cur.clone() {
            Value::Unit => None,
            Value::Int(_)
            | Value::Str(_)
            | Value::Float(_)
            | Value::BuiltinFn(_)
            | Value::Ident(_)
            | Value::Closure { .. } => Some(self.cur.clone()),

            Value::Cons { head: hd, tail: tl } => {
                self.cur = tl.clone();
                Some(hd.clone())
            }
        }
    }
}

impl From<Sexp> for Value {
    fn from(sexp: Sexp) -> Self {
        match sexp {
            Sexp::Unit => Value::Unit,
            Sexp::Atom(ref atm) => match atm {
                Atomic::Int(n) => Value::Int(*n),
                Atomic::Float(n) => Value::Float(*n),
                Atomic::Ident(s) => Value::Ident(s.clone()),
                Atomic::Str(s) => Value::Str(s.clone()),
            },
            Sexp::Exp { ref head, ref tail } => {
                let hd: Value = head.clone().into();
                let tl: Value = tail.clone().into();
                Value::Cons {
                    head: hd.into(),
                    tail: tl.into(),
                }
            }
        }
    }
}

impl From<Rc<Sexp>> for Value {
    fn from(sexp: Rc<Sexp>) -> Self {
        match *sexp {
            Sexp::Unit => Value::Unit,
            Sexp::Atom(ref atm) => match atm {
                Atomic::Int(n) => Value::Int(*n),
                Atomic::Float(n) => Value::Float(*n),
                Atomic::Ident(s) => Value::Ident(s.clone()),
                Atomic::Str(s) => Value::Str(s.clone()),
            },
            Sexp::Exp { ref head, ref tail } => {
                let hd: Value = head.clone().into();
                let tl: Value = tail.clone().into();
                Value::Cons {
                    head: hd.into(),
                    tail: tl.into(),
                }
            }
        }
    }
}
