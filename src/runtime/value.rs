use std::{
    cell::RefCell,
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

/// A runtime value. Most variants are data (and double as the AST the
/// runtime walks); `NativeFn`, `Closure`, and `Macro` are callable.
///
/// Lists are `Cons` chains terminated by `Unit`, mirroring
/// [`crate::parser::Sexp`]. `Array` and `Map` mirror
/// [`crate::parser::Collection`]. Floats are wrapped in [`OrderedFloat`] so
/// `Value` can be `Hash + Eq` and thus serve as a `Map` key. `Ref` is the
/// only mutable variant — every other variant is logically immutable, with
/// builtins returning new values (collections share structure under the
/// hood via `im`).
///
/// `Value` derives `Clone` but the underlying allocations are `Rc`-backed,
/// so cloning is cheap; copy `Value` freely.
///
/// # Constructing values from Rust
///
/// Many `From` impls are provided so host code can hand off Rust values
/// directly:
///
/// ```
/// use rizz::runtime::Value;
///
/// let _: Value = 42i64.into();          // Value::Int
/// let _: Value = 3.14f64.into();        // Value::Float
/// let _: Value = "hi".into();           // Value::Str
/// let _: Value = true.into();           // Value::Int(1) — booleans encode as ints
/// let _: Value = vec![1i64, 2].into();  // Value::Array
/// let _: Value = ().into();             // Value::Unit
/// let _: Value = Option::<i64>::None.into(); // Value::Unit
/// ```
#[derive(Clone)]
pub enum Value {
    /// A UTF-8 string.
    Str(Rc<str>),
    /// A 64-bit signed integer.
    Int(i64),
    /// A 64-bit IEEE-754 float; ordered for hashing/equality (all NaNs
    /// compare equal).
    Float(OrderedFloat<f64>),
    /// An identifier — typically produced by `(quote x)`. Bare idents in
    /// source resolve via the env and don't appear as runtime values.
    Ident(Rc<str>),
    /// The unit value, also the empty list `()` (nil). Falsy.
    Unit,
    /// A cons cell `(head . tail)`. Lists are cons chains terminated by
    /// `Unit`; dotted (improper) lists end in some other value.
    Cons { head: Rc<Value>, tail: Rc<Value> },
    /// A Rust-implemented function. See [`NativeFn`] for the four flavors.
    NativeFn(Rc<NativeFn>),
    /// A user-defined function captured at its definition site.
    Closure(Rc<Closure>),
    /// A user-defined macro. Structurally a [`Closure`], but at a call
    /// site its arguments are passed **unevaluated**; the body produces a
    /// form that is then evaluated in the caller's environment.
    ///
    /// Note: only the expansion's *value* propagates — any env extension
    /// from evaluating the expansion is discarded, so a macro expanding to
    /// `(let x 1)` does **not** bind `x` at the call site. Macros that need
    /// to introduce state should expand to `ref` mutations instead.
    Macro(Rc<Closure>),
    /// A persistent vector. Cheap to clone and update.
    Array(Vector<Rc<Value>>),
    /// A persistent hash map keyed by any [`Value`].
    Map(HashMap<Rc<Value>, Rc<Value>>),
    /// A mutable cell — the only path to in-place mutation in rizz.
    /// Multiple bindings of the same ref share its cell.
    Ref(Rc<RefCell<Value>>),
}

impl Drop for Value {
    /// Unlinks the cons spine iteratively so that dropping a long list does
    /// not recurse once per element (the derived drop would overflow the
    /// stack on lists tens of thousands of elements long). Nested heads and
    /// other variants drop normally.
    fn drop(&mut self) {
        let Value::Cons { tail, .. } = self else {
            return;
        };
        let mut cur = std::mem::replace(tail, Rc::new(Value::Unit));
        // Each owned node has its tail snipped before it drops, so its own
        // `drop` terminates immediately. A shared tail (`Err`) is left for
        // its remaining owners.
        while let Ok(mut node) = Rc::try_unwrap(cur) {
            match &mut node {
                Value::Cons { tail, .. } => cur = std::mem::replace(tail, Rc::new(Value::Unit)),
                _ => break,
            }
        }
    }
}

/// A user-defined function. Backs both [`Value::Closure`] and
/// [`Value::Macro`] — the difference is in how the call site treats the
/// arguments (evaluated for closures, raw for macros).
///
/// # Fields
///
/// - `name` — the function's own identifier. Bound inside the body so the
///   function can recurse; empty for anonymous closures.
/// - `params` — positional parameter names.
/// - `rest` — `Some(name)` for variadic functions declared with a
///   dotted-tail param list `(a b . rest)` (or a bare-ident param list,
///   which is shorthand for "zero positional params, rest is everything").
///   At a call site the trailing arguments past `params.len()` are bundled
///   into a cons list and bound under that name. `None` for fixed arity.
/// - `body` — a single form. Multi-step bodies use `do`.
/// - `env` — the lexical env captured at definition. Closures capture by
///   snapshot, not by reference, so later rebindings in the outer env
///   don't affect what the body sees (refs are the explicit escape hatch
///   for shared mutable state).
/// - `doc` — optional documentation attached via the `(doc ...)` slot at
///   definition. Surfaced by the `show` builtin.
#[derive(Clone, PartialEq)]
pub struct Closure {
    pub name: Rc<str>,
    pub params: Vec<Rc<str>>,
    pub rest: Option<Rc<str>>,
    pub body: Rc<Value>,
    pub env: Env,
    pub doc: Option<Rc<str>>,
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

    /// The variant's name, for use in error messages and the `typeof` builtin.
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
            Self::Ref(_) => "ref",
        }
    }

    /// Builds a cons list from already-wrapped values, terminated by `Unit`.
    /// This is the canonical list constructor — the evaluator, `quasi`, and
    /// the prelude all build lists through it.
    pub fn list_of<I>(items: I) -> Value
    where
        I: IntoIterator<Item = Rc<Value>>,
        I::IntoIter: DoubleEndedIterator,
    {
        items
            .into_iter()
            .rfold(Value::Unit, |tail, head| Value::Cons {
                head,
                tail: Rc::new(tail),
            })
    }

    /// Build a cons list out of `xs`, terminated by `Unit`. Each element
    /// is converted into a [`Value`] via its `Into` impl, so any sequence
    /// of types that convert into `Value` can be turned into a list:
    ///
    /// ```
    /// use rizz::runtime::Value;
    /// let v = Value::cons_of(vec![1i64, 2, 3]);
    /// // structurally equivalent to (quote (1 2 3))
    /// # assert!(matches!(v, Value::Cons { .. }));
    /// ```
    pub fn cons_of<T>(xs: impl IntoIterator<Item = T, IntoIter: DoubleEndedIterator>) -> Value
    where
        T: Into<Value> + Clone,
    {
        Self::list_of(xs.into_iter().map(|item| Rc::new(item.into())))
    }

    /// Whether the value counts as true in a condition. Everything is
    /// truthy except: `Unit`, `Int(0)`, `Float(0.0)`, the empty string,
    /// the empty identifier, `[]`, `{}`, and any [`Ref`](Self::Ref)
    /// whose current contents are falsy.
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
            Self::Ref(v) => v.borrow().is_truthy(),
        }
    }

    /// Whether the value can appear in head position of a call.
    /// True for [`NativeFn`](Self::NativeFn), [`Closure`](Self::Closure),
    /// and any [`Ref`](Self::Ref) whose cell ultimately holds one of those.
    pub fn is_callable(&self) -> bool {
        match self {
            Value::NativeFn(_) | Value::Closure(_) => true,
            Value::Ref(v) => v.borrow().is_callable(),
            _ => false,
        }
    }

    /// Whether the value is `Unit` (or a ref ultimately holding `Unit`).
    pub fn is_unit(&self) -> bool {
        match self {
            Value::Unit => true,
            Value::Ref(v) => v.borrow().is_unit(),
            _ => false,
        }
    }

    /// Whether the value is `Int` or `Float` (or a ref ultimately holding one).
    pub fn is_numeric(&self) -> bool {
        match self {
            Value::Float(_) | Value::Int(_) => true,
            Value::Ref(v) => v.borrow().is_numeric(),
            _ => false,
        }
    }

    /// `Some(n)` for `Int(n)` or a ref-chain ending in one; `None` otherwise.
    /// The `as_*` family follows this same "peel refs, match variant" shape.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Ref(v) => v.borrow().as_int(),
            _ => None,
        }
    }

    /// `Some(f)` for `Float(f)` or a ref-chain ending in one.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(n) => Some(n.0),
            Value::Ref(v) => v.borrow().as_float(),
            _ => None,
        }
    }

    /// `Some(s)` for `Str(s)` or a ref-chain ending in one. Use
    /// [`as_str_or_ident`](Self::as_str_or_ident) when an ident should
    /// also be accepted (e.g. `(open ident)`).
    pub fn as_str(&self) -> Option<Rc<str>> {
        match self {
            Value::Str(s) => Some(s.clone()),
            Value::Ref(v) => v.borrow().as_str(),
            _ => None,
        }
    }

    /// `Some(s)` for `Str(s)` or `Ident(s)` (or a ref-chain ending in
    /// either).
    pub fn as_str_or_ident(&self) -> Option<Rc<str>> {
        match self {
            Value::Str(s) | Value::Ident(s) => Some(s.clone()),
            Value::Ref(v) => v.borrow().as_str(),
            _ => None,
        }
    }

    /// `Some(xs)` for `Array(xs)` or a ref-chain ending in one.
    pub fn as_array(&self) -> Option<Vector<Rc<Value>>> {
        match self {
            Value::Array(xs) => Some(xs.clone()),
            Value::Ref(v) => v.borrow().as_array(),
            _ => None,
        }
    }

    /// `Some(m)` for `Map(m)` or a ref-chain ending in one.
    pub fn as_map(&self) -> Option<HashMap<Rc<Value>, Rc<Value>>> {
        match self {
            Value::Map(xs) => Some(xs.clone()),
            Value::Ref(v) => v.borrow().as_map(),
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
            Value::Ref(v) => v.borrow().repr(),
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
            (Value::Ref(a), Value::Ref(b)) => Rc::ptr_eq(a, b),
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
            Value::Ref(_) | Value::NativeFn(_) | Value::Closure(_) | Value::Macro(_) => {}
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
            Value::Ref(v) => write!(f, "<ref {}>", v.borrow()),
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

impl From<&Sexp> for Value {
    fn from(sexp: &Sexp) -> Self {
        match sexp {
            Sexp::Unit => Value::Unit,
            Sexp::Atom(atm) => atm.into(),
            Sexp::Exp { .. } => {
                // Walk the cons spine iteratively so converting a long list
                // recurses per *nesting level* (bounded by the parser), not
                // per element.
                let mut heads: Vec<Rc<Value>> = Vec::new();
                let mut cur = sexp;
                while let Sexp::Exp { head, tail } = cur {
                    heads.push(Rc::new(Value::from(&**head)));
                    cur = tail;
                }
                let last_tail = Value::from(cur);
                heads
                    .into_iter()
                    .rfold(last_tail, |tail, head| Value::Cons {
                        head,
                        tail: Rc::new(tail),
                    })
            }
            Sexp::Collection(c) => c.into(),
        }
    }
}

impl From<Sexp> for Value {
    fn from(sexp: Sexp) -> Self {
        (&sexp).into()
    }
}

impl From<Rc<Sexp>> for Value {
    fn from(sexp: Rc<Sexp>) -> Self {
        (&*sexp).into()
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
