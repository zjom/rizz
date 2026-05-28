use std::{fmt::Debug, rc::Rc};

use crate::{
    Env,
    evaluator::EvaluatorError,
    parser::{Atomic, Sexp},
};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------
pub type BuiltinFn = Rc<dyn Fn(&[Rc<Value>], &Env) -> Result<(Rc<Value>, Env), EvaluatorError>>;

#[derive(Clone)]
pub enum Value {
    Str(Rc<str>),
    Int(i64),
    Float(f64),
    Ident(Rc<str>),
    Unit,
    Cons { head: Rc<Value>, tail: Rc<Value> },
    BuiltinFn(BuiltinFn),
    Closure(Rc<Closure>),
}

#[derive(Clone, PartialEq)]
pub struct Closure {
    pub name: Rc<str>,
    pub params: Vec<Rc<str>>,
    pub body: Rc<Value>,
    pub env: Env,
}

// ---------------------------------------------------------------------------
// Value methods
// ---------------------------------------------------------------------------

impl Value {
    pub fn iter(value: &Rc<Value>) -> impl Iterator<Item = Rc<Value>> {
        Iter::new(value.clone())
    }

    pub fn type_name(v: &Value) -> &'static str {
        match v {
            Self::Str(_) => "str",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Ident(_) => "ident",
            Self::Unit => "()",
            Self::Cons { .. } => "cons",
            Self::BuiltinFn(_) => "builtin",
            Self::Closure(_) => "closure",
        }
    }

    // --- Type predicates ---

    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Str(s) => !s.is_empty(),
            Self::Int(n) => *n != 0,
            Self::Float(n) => *n != 0.,
            Self::Ident(s) => !s.is_empty(),
            Self::Unit => false,
            Self::BuiltinFn(_) => true,
            Self::Closure(_) => true,
            Self::Cons { head, .. } => !matches!(&**head, Value::Unit),
        }
    }

    pub fn is_callable(&self) -> bool {
        matches!(self, Value::BuiltinFn(_) | Value::Closure(_))
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, Value::Unit)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Float(_) | Value::Int(_))
    }

    // --- Accessors ---

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<Rc<str>> {
        match self {
            Value::Str(s) | Value::Ident(s) => Some(s.clone()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// PartialEq
// ---------------------------------------------------------------------------

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Ident(a), Value::Ident(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Cons { head: h1, tail: t1 }, Value::Cons { head: h2, tail: t2 }) => {
                h1 == h2 && t1 == t2
            }
            // Functions are not comparable
            (Value::BuiltinFn(_), Value::BuiltinFn(_)) => false,
            (Value::Closure(a), Value::Closure(b)) => a == b,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Numeric trait
// ---------------------------------------------------------------------------

pub trait Numeric: Sized + Copy {
    fn from_value(v: &Value) -> Option<Self>;
    fn into_value(self) -> Value;
    const TYPE_NAME: &'static str;
}

impl Numeric for i64 {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_int()
    }
    fn into_value(self) -> Value {
        Value::Int(self)
    }
    const TYPE_NAME: &'static str = "int";
}

impl Numeric for f64 {
    fn from_value(v: &Value) -> Option<Self> {
        v.as_float()
    }
    fn into_value(self) -> Value {
        Value::Float(self)
    }
    const TYPE_NAME: &'static str = "float";
}

// ---------------------------------------------------------------------------
// Debug
// ---------------------------------------------------------------------------

const MAX_DEBUG_DEPTH: usize = 4;

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DepthLimited {
            value: self,
            depth: MAX_DEBUG_DEPTH,
        }
        .fmt(f)
    }
}

/// Formats a `Value` with a remaining-depth budget to avoid unbounded output.
struct DepthLimited<'a> {
    value: &'a Value,
    depth: usize,
}

impl Debug for DepthLimited<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.value {
            Value::Str(s) => write!(f, "<str=\"{s}\">"),
            Value::Ident(s) => write!(f, "<ident={s}>"),
            Value::Float(n) => write!(f, "<float={n}>"),
            Value::Int(n) => write!(f, "<int={n}>"),
            Value::Unit => write!(f, "<()>"),
            Value::BuiltinFn(_) => write!(f, "<builtin_fn>"),
            Value::Closure(c) => write!(f, "<closure params={:?}>", c.params),
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

// ---------------------------------------------------------------------------
// Iterator
// ---------------------------------------------------------------------------

pub struct Iter {
    cur: Rc<Value>,
}

impl Iter {
    pub fn new(value: Rc<Value>) -> Self {
        Iter { cur: value }
    }
}

impl Iterator for Iter {
    type Item = Rc<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        match &*self.cur.clone() {
            Value::Unit => None,
            Value::Cons { head, tail } => {
                self.cur = tail.clone();
                Some(head.clone())
            }
            // Leaf values yield themselves once
            _ => {
                let val = self.cur.clone();
                self.cur = Rc::new(Value::Unit);
                Some(val)
            }
        }
    }
}

impl IntoIterator for Value {
    type Item = Rc<Value>;
    type IntoIter = Iter;

    fn into_iter(self) -> Self::IntoIter {
        Iter { cur: Rc::new(self) }
    }
}

// ---------------------------------------------------------------------------
// From conversions
// ---------------------------------------------------------------------------

impl From<Sexp> for Value {
    fn from(sexp: Sexp) -> Self {
        match sexp {
            Sexp::Unit => Value::Unit,
            Sexp::Atom(ref atm) => atm_to_value(atm),
            Sexp::Exp { ref head, ref tail } => Value::Cons {
                head: Rc::new(Value::from(head.clone())),
                tail: Rc::new(Value::from(tail.clone())),
            },
        }
    }
}

impl From<Rc<Sexp>> for Value {
    fn from(sexp: Rc<Sexp>) -> Self {
        match *sexp {
            Sexp::Unit => Value::Unit,
            Sexp::Atom(ref atm) => atm_to_value(atm),
            Sexp::Exp { ref head, ref tail } => Value::Cons {
                head: Rc::new(Value::from(head.clone())),
                tail: Rc::new(Value::from(tail.clone())),
            },
        }
    }
}

fn atm_to_value(atm: &Atomic) -> Value {
    match atm {
        Atomic::Int(n) => Value::Int(*n),
        Atomic::Float(n) => Value::Float(*n),
        Atomic::Ident(s) => Value::Ident(s.clone()),
        Atomic::Str(s) => Value::Str(s.clone()),
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Float(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Int(b as i64)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s.into())
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.into())
    }
}

impl From<Rc<str>> for Value {
    fn from(s: Rc<str>) -> Self {
        Value::Str(s)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Unit
    }
}

impl From<BuiltinFn> for Value {
    fn from(f: BuiltinFn) -> Self {
        Value::BuiltinFn(f)
    }
}

impl From<Closure> for Value {
    fn from(value: Closure) -> Self {
        Value::Closure(Rc::new(value))
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => v.into(),
            None => Value::Unit,
        }
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(vec: Vec<T>) -> Self {
        vec.into_iter()
            .rfold(Value::Unit, |tail, item| Value::Cons {
                head: Rc::new(item.into()),
                tail: Rc::new(tail),
            })
    }
}
