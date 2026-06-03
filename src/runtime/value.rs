use std::{
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    rc::Rc,
};

use im::{HashMap, Vector};
use ordered_float::OrderedFloat;

use crate::{
    Env,
    parser::{Atomic, Collection, Sexp},
    runtime::NativeFn,
};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A runtime value. Most variants are data (and double as the AST the runtime
/// walks); `NativeFn` and `Closure` are the two kinds of callable.
///
/// Lists are `Cons` chains terminated by `Unit`, mirroring [`crate::parser::Sexp`].
/// `Array` and `Map` mirror [`crate::parser::Collection`]. Floats are wrapped in
/// [`OrderedFloat`] so that `Value` can be `Hash + Eq` and thus serve as a `Map`
/// key.
#[derive(Clone)]
pub enum Value {
    Str(Rc<str>),
    Int(i64),
    Float(OrderedFloat<f64>),
    Ident(Rc<str>),
    Unit,
    Cons {
        head: Rc<Value>,
        tail: Rc<Value>,
    },
    NativeFn(Rc<NativeFn>),
    Closure(Rc<Closure>),
    /// A user-defined macro. Structurally a [`Closure`], but at a call site its
    /// arguments are passed unevaluated; the body produces a form that is then
    /// evaluated in the caller's environment.
    Macro(Rc<Closure>),
    Array(Vector<Rc<Value>>),
    Map(HashMap<Rc<Value>, Rc<Value>>),
}

/// A user-defined function: its `name`, parameter names, body form, and the
/// `env` captured where it was defined (lexical scope). The name lets the body
/// refer to itself, which is what enables recursion.
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
    /// Iterates the elements of a cons list. A non-list value yields itself
    /// once (see [`Iter`]).
    pub fn iter(value: &Rc<Value>) -> impl Iterator<Item = Rc<Value>> {
        Iter::new(value.clone())
    }

    /// The variant's name, for use in error messages.
    pub fn type_name(v: &Value) -> &'static str {
        match v {
            Self::Str(_) => "str",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Ident(_) => "ident",
            Self::Unit => "()",
            Self::Cons { .. } => "cons",
            Self::NativeFn(_) => "native",
            Self::Closure(_) => "closure",
            Self::Macro(_) => "macro",
            Self::Array(_) => "array",
            Self::Map(_) => "map",
        }
    }

    // --- Constructors ---
    pub fn cons_of<T>(xs: impl IntoIterator<Item = T, IntoIter: DoubleEndedIterator>) -> Value
    where
        T: Into<Value> + Clone,
    {
        xs.into_iter().rfold(Value::Unit, |tail, item| Value::Cons {
            head: Rc::new(item.into()),
            tail: Rc::new(tail),
        })
    }

    // --- Type predicates ---

    /// Whether the value counts as true in a condition. Everything is truthy
    /// except `Unit`, zero numbers, and empty strings/identifiers.
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Str(s) => !s.is_empty(),
            Self::Int(n) => *n != 0,
            Self::Float(n) => n.0 != 0.,
            Self::Ident(s) => !s.is_empty(),
            Self::Unit => false,
            Self::NativeFn(_) => true,
            Self::Closure(_) => true,
            Self::Macro(_) => true,
            Self::Cons { .. } => true,
            Self::Array(xs) => !xs.is_empty(),
            Self::Map(m) => !m.is_empty(),
        }
    }

    pub fn is_callable(&self) -> bool {
        matches!(self, Value::NativeFn(_) | Value::Closure(_))
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
            Value::Float(n) => Some(n.0),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<Rc<str>> {
        match self {
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<Vector<Rc<Value>>> {
        match self {
            Value::Array(xs) => Some(xs.clone()),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<HashMap<Rc<Value>, Rc<Value>>> {
        match self {
            Value::Map(xs) => Some(xs.clone()),
            _ => None,
        }
    }

    /// Renders the value as a string for `to-str`. Top-level strings and
    /// identifiers render as their raw content (no quotes); everything else
    /// matches [`repr`](Self::repr).
    pub fn display(&self) -> String {
        match self {
            Value::Str(s) | Value::Ident(s) => s.to_string(),
            _ => self.repr(),
        }
    }

    /// Like [`display`](Self::display), but strings are quoted so that values
    /// nested inside collections stay readable.
    pub fn repr(&self) -> String {
        match self {
            Value::Str(s) => format!("\"{s}\""),
            Value::Ident(s) => s.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => n.to_string(),
            Value::Unit => "()".to_string(),
            Value::Array(xs) => {
                let inner: Vec<String> = xs.iter().map(|x| x.repr()).collect();
                format!("[{}]", inner.join(" "))
            }
            Value::Map(m) => {
                let inner: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.repr(), v.repr()))
                    .collect();
                format!("{{{}}}", inner.join(" "))
            }
            Value::Cons { .. } => {
                let inner: Vec<String> = Value::iter(&Rc::new(self.clone()))
                    .map(|x| x.repr())
                    .collect();
                format!("({})", inner.join(" "))
            }
            Value::NativeFn(_) | Value::Closure(_) => "<fn>".to_string(),
            Value::Macro(_) => "<macro>".to_string(),
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
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            // Native fns compare by identity: distinct ones are never equal,
            // but one equals itself (required for `Eq` reflexivity, which in
            // turn lets `Value` key a `Map`).
            (Value::NativeFn(a), Value::NativeFn(b)) => Rc::ptr_eq(a, b),
            (Value::Closure(a), Value::Closure(b)) => a == b,
            (Value::Macro(a), Value::Macro(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Str(s) | Value::Ident(s) => s.hash(state),
            Value::Int(n) => n.hash(state),
            Value::Float(n) => n.hash(state),
            Value::Unit => {}
            Value::Cons { head, tail } => {
                head.hash(state);
                tail.hash(state);
            }
            Value::Array(xs) => {
                for x in xs.iter() {
                    x.hash(state);
                }
            }
            // A map's iteration order is unspecified, so fold entries with a
            // commutative (XOR) combiner to keep the hash order-independent.
            Value::Map(m) => {
                let mut acc: u64 = 0;
                for (k, v) in m.iter() {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    k.hash(&mut h);
                    v.hash(&mut h);
                    acc ^= h.finish();
                }
                acc.hash(state);
            }
            // Callables hash by discriminant only. Equal callables (same Rc, or
            // structurally-equal closures) share that hash; collisions between
            // distinct ones are allowed.
            Value::NativeFn(_) | Value::Closure(_) | Value::Macro(_) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Numeric trait
// ---------------------------------------------------------------------------

/// Bridges Rust's numeric types (`i64`, `f64`) to [`Value`], letting the
/// arithmetic builtins in [`crate::prelude::numbers`] be written generically
/// over both. `TYPE_NAME` is used in type-mismatch error messages.
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
        Value::Float(OrderedFloat(self))
    }
    const TYPE_NAME: &'static str = "float";
}

// ---------------------------------------------------------------------------
// Debug/ Display
// ---------------------------------------------------------------------------

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

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
            Value::NativeFn(_) => write!(f, "<native_fn>"),
            Value::Closure(c) => write!(f, "<closure params={:?}>", c.params),
            Value::Macro(c) => write!(f, "<macro params={:?}>", c.params),
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
            Value::Array(xs) => {
                if self.depth == 0 {
                    return write!(f, "<array ...>");
                }
                let next = self.depth - 1;
                write!(f, "<array")?;
                for x in xs.iter() {
                    write!(
                        f,
                        " {:?}",
                        DepthLimited {
                            value: x,
                            depth: next
                        }
                    )?;
                }
                write!(f, ">")
            }
            Value::Map(m) => {
                if self.depth == 0 {
                    return write!(f, "<map ...>");
                }
                let next = self.depth - 1;
                write!(f, "<map")?;
                for (k, v) in m.iter() {
                    write!(
                        f,
                        " {:?}:{:?}",
                        DepthLimited {
                            value: k,
                            depth: next
                        },
                        DepthLimited {
                            value: v,
                            depth: next
                        }
                    )?;
                }
                write!(f, ">")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Iterator
// ---------------------------------------------------------------------------

/// Walks a cons list, yielding each `head`. A `Unit` terminates iteration; a
/// non-list (leaf) value yields itself once, which lets callers treat a lone
/// argument and a one-element list uniformly.
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
            Sexp::Atom(ref atm) => atm.into(),
            Sexp::Exp { ref head, ref tail } => Value::Cons {
                head: Rc::new(Value::from(head.clone())),
                tail: Rc::new(Value::from(tail.clone())),
            },
            Sexp::Collection(ref c) => c.into(),
        }
    }
}

impl From<Rc<Sexp>> for Value {
    fn from(sexp: Rc<Sexp>) -> Self {
        match *sexp {
            Sexp::Unit => Value::Unit,
            Sexp::Atom(ref atm) => atm.into(),
            Sexp::Exp { ref head, ref tail } => Value::Cons {
                head: Rc::new(Value::from(head.clone())),
                tail: Rc::new(Value::from(tail.clone())),
            },
            Sexp::Collection(ref c) => c.into(),
        }
    }
}

impl From<&Atomic> for Value {
    fn from(atm: &Atomic) -> Self {
        match atm {
            Atomic::Int(n) => Value::Int(*n),
            Atomic::Float(n) => Value::Float(*n),
            Atomic::Ident(s) => Value::Ident(s.clone()),
            Atomic::Str(s) => Value::Str(s.clone()),
        }
    }
}

impl From<&Collection> for Value {
    fn from(c: &Collection) -> Self {
        match c {
            Collection::Array(xs) => {
                Value::Array(xs.iter().map(|s| Rc::new(Value::from(s.clone()))).collect())
            }
            Collection::Map(m) => {
                let entries = m.iter().map(|(k, v)| {
                    (
                        Rc::new(Value::from(k.clone())),
                        Rc::new(Value::from(v.clone())),
                    )
                });
                Value::Map(entries.collect())
            }
        }
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Float(OrderedFloat(n))
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

impl From<&Path> for Value {
    fn from(value: &Path) -> Self {
        let s: Rc<str> = value.to_string_lossy().into();
        Value::Str(s)
    }
}

impl From<PathBuf> for Value {
    fn from(value: PathBuf) -> Self {
        let s: &Path = value.as_ref();
        s.into()
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Unit
    }
}

impl From<NativeFn> for Value {
    fn from(f: NativeFn) -> Self {
        Value::NativeFn(Rc::new(f))
    }
}

impl From<Closure> for Value {
    fn from(value: Closure) -> Self {
        Value::Closure(Rc::new(value))
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(value: Vec<T>) -> Self {
        Value::Array(value.into_iter().map(|v| Rc::new(v.into())).collect())
    }
}

impl<T: Into<Value> + Clone> From<Vector<T>> for Value {
    fn from(value: Vector<T>) -> Self {
        Value::Array(value.into_iter().map(|v| Rc::new(v.into())).collect())
    }
}

impl<K: Into<Value> + Clone + Hash + Eq, V: Into<Value> + Clone>
    From<std::collections::HashMap<K, V>> for Value
{
    fn from(value: std::collections::HashMap<K, V>) -> Self {
        let m = value
            .into_iter()
            .map(|(k, v)| (Rc::new(k.into()), Rc::new(v.into())))
            .collect();
        Value::Map(m)
    }
}

impl<K: Into<Value> + Clone + Hash + Eq, V: Into<Value> + Clone> From<HashMap<K, V>> for Value {
    fn from(value: HashMap<K, V>) -> Self {
        let m = value
            .into_iter()
            .map(|(k, v)| (Rc::new(k.into()), Rc::new(v.into())))
            .collect();
        Value::Map(m)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_unquotes_top_level_strings() {
        assert_eq!(Value::Str("hi".into()).display(), "hi");
        assert_eq!(Value::Int(42).display(), "42");
        assert_eq!(Value::Unit.display(), "()");
    }

    #[test]
    fn repr_quotes_strings_and_formats_collections() {
        assert_eq!(Value::Str("hi".into()).repr(), "\"hi\"");
        let arr = Value::Array(
            vec![Rc::new(Value::Int(1)), Rc::new(Value::Str("a".into()))]
                .into_iter()
                .collect(),
        );
        assert_eq!(arr.display(), "[1 \"a\"]");
    }
}
