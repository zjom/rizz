//! Reflection builtins: `typeof`, `show`, `id`, `empty-of`.
//!
//! - `typeof` returns the type name of a value as an ident, matching
//!   [`Value::type_name`].
//! - `show` retrieves documentation attached at definition time (via the
//!   `(doc "...")` slot on `let`/`let!`/`fn`/`defmacro`). For an ident
//!   naming a special form, it returns the built-in description of that
//!   form so end users can do `(show 'fn)` to look up syntax.
//! - `id` is the identity function — handy as a no-op argument to
//!   higher-order operations like `compose`.
//! - `empty-of` returns an "empty" value of the same variant as its
//!   argument (0 for ints, "" for strings, [] for arrays, etc.), peeling
//!   through refs.

use crate::{
    Env, RuntimeError,
    consts::{
        KW_DEFMACRO, KW_DO, KW_DOC, KW_EVAL, KW_FN, KW_IF, KW_LET, KW_LET_REF, KW_LOAD,
        KW_LOAD_QUOTED, KW_OPEN, KW_QUASIQUOTE, KW_QUOTE, KW_UNQUOTE, KW_UNQUOTE_SPLICE,
    },
    runtime::{Closure, NativeFn, Value},
};
use im::{HashMap, Vector};
use std::rc::Rc;

/// All reflection builtins: `typeof`, `show`, `id`, `empty-of`.
pub fn env() -> Env {
    Env::of_builtins(vec![
        ("typeof", typeof_()),
        ("show", show()),
        ("id", id()),
        ("empty-of", empty_of()),
        ("is", is()),
    ])
}

/// `(typeof v)`: the variant name of `v` as an ident (e.g. `'int`, `'cons`).
fn typeof_() -> NativeFn {
    NativeFn::pure("typeof".into(), 1, |args| {
        Ok(Rc::new(Value::Ident(Value::type_name(&args[0]).into())))
    })
    .with_doc(
        "\
(typeof V)

Returns ident: the type name of V — one of 'int 'float 'str
'ident 'array 'map 'ref 'cons 'closure 'macro 'native-fn 'unit.

See also: (is X TY), (empty-of V)."
            .into(),
    )
}

/// `(is x 'map)`
fn is() -> NativeFn {
    let name: Rc<str> = "is".into();
    NativeFn::pure(name.clone(), 2, move |args| {
        let expected = args[1]
            .as_str_or_ident()
            .ok_or_else(|| RuntimeError::type_mismatch("is", "ident", &args[1]))?;
        let actual = Value::type_name(&args[0]);
        if expected.as_ref().trim() == actual {
            return Ok(args[0].clone());
        }

        Ok(Rc::new(Value::Unit))
    })
    .with_doc(
        "\
(is X TY)

Returns X if X's type name matches TY, else ().

TY — ident | str: a type name as reported by (typeof V).

Example:
  (is 5 'int)   ;; => 5
  (is 5 'str)   ;; => ()

See also: (typeof V)."
            .into(),
    )
}

/// `(id v)`: identity function — returns `v` unchanged.
fn id() -> NativeFn {
    NativeFn::pure("id".into(), 1, |args| Ok(args[0].clone())).with_doc(
        "\
(id V)

Returns V unchanged — the identity function, handy as a no-op
argument to higher-order operations like (compose F G)."
            .into(),
    )
}

/// `(show v)`: returns the doc string attached to a closure, macro, or native
/// fn at definition time (see the optional `(doc "...")` slot on binding
/// forms). When `v` is an identifier naming a special form (e.g. `'fn`,
/// `'let!`, `'defmacro`), returns the built-in description of that form. When
/// `v` is any other ident, it is looked up in the env and the bound value's
/// doc is returned. Returns `()` when no doc is available. Refs are peeled so
/// `(show (deref r))` and `(show r)` behave the same.
fn show() -> NativeFn {
    NativeFn::with_env("show".into(), 1, |args, env| {
        let v = args[0].clone();
        let doc = match &*v {
            Value::Ident(name) => special_form_doc(name)
                .map(Rc::from)
                .or_else(|| env.get(name).and_then(|bound| doc_of(bound))),
            _ => doc_of(&v),
        };
        let out = match doc {
            Some(s) => Rc::new(Value::Str(s)),
            None => Rc::new(Value::Unit),
        };
        Ok(out)
    })
    .with_doc(
        "\
(show V)

Returns the doc string attached to a closure, macro, or native fn
at definition time (see the optional (doc \"...\") slot on binding
forms). When V is an ident naming a special form (e.g. 'fn, 'let!,
'quote), returns the built-in description of that form; any other
ident is looked up in the env and the bound value's doc is
returned. Returns () when no doc is available. Refs are peeled, so
(show (deref r)) and (show r) behave the same.

Example:
  (show 'fn)   ;; => the fn special form's syntax help"
            .into(),
    )
}

/// `(empty-of v)`: returns an "empty" value of the same variant as `v` —
/// `0` for ints, `0.0` for floats, `""` for strings, the empty ident for
/// idents, `()` for cons/unit, an empty array/map for arrays/maps, and a
/// nullary `() -> ()` callable for closures/macros/native fns. Refs are
/// peeled: the result mirrors the inner value's variant and is not
/// re-wrapped in a ref.
fn empty_of() -> NativeFn {
    NativeFn::pure("empty-of".into(), 1, |args| {
        Ok(Rc::new(empty_of_aux(&args[0])))
    })
    .with_doc(
        "\
(empty-of V)

Returns an empty value of the same variant as V: 0 for ints, 0.0
for floats, \"\" for strs, the empty ident for idents, () for
cons/unit, [] for arrays, {} for maps, and a nullary () -> ()
callable for closures, macros, and native fns. Refs are peeled —
the result mirrors the inner value's variant and is not re-wrapped
in a ref.

See also: (typeof V)."
            .into(),
    )
}

fn empty_of_aux(value: &Value) -> Value {
    let unit_closure = Rc::new(Closure {
        name: "".into(),
        params: vec![],
        rest: None,
        body: Rc::new(Value::Unit),
        env: Env::new(),
        doc: None,
    });
    match value {
        Value::Str(_) => "".into(),
        Value::Int(_) => 0.into(),
        Value::Float(_) => 0f64.into(),
        Value::Ident(_) => Value::Ident("".into()),
        Value::Cons { .. } | Value::Unit => Value::Unit,
        Value::NativeFn(_) => Value::NativeFn(Rc::new(unit())),
        Value::Closure(_) => Value::Closure(unit_closure.clone()),
        Value::Macro(_) => Value::Macro(unit_closure.clone()),
        Value::Array(_) => Value::Array(Vector::new()),
        Value::Map(_) => Value::Map(HashMap::new()),
        Value::Ref(refc) => empty_of_aux(&refc.borrow().clone()),
    }
}

fn unit() -> NativeFn {
    NativeFn::pure("".into(), 0, |_| Ok(Rc::new(Value::Unit)))
}

fn doc_of(v: &Value) -> Option<Rc<str>> {
    match v {
        Value::Closure(c) | Value::Macro(c) => c.doc.clone(),
        Value::NativeFn(n) => n.doc(),
        Value::Ref(cell) => doc_of(&cell.borrow()),
        _ => None,
    }
}

fn special_form_doc(name: &str) -> Option<&'static str> {
    Some(match name {
        KW_LET => {
            "\
(let NAME VALUE)
(let NAME (doc STR+) VALUE)

Evaluates VALUE, binds it to NAME in the surrounding env, and
returns the bound value. An optional (doc \"...\") slot documents
the binding; the doc is attached to callables (closure, macro,
native fn) and silently dropped on non-callables.

Errors when called with arity \u{2260} 2 (or 3 with a doc slot), or when
NAME is not an ident.

See also: (let! NAME VALUE), (fn NAME PARAMS BODY), (show V)."
        }

        KW_LET_REF => {
            "\
(let! NAME VALUE)
(let! NAME (doc STR+) VALUE)

Like (let NAME VALUE), but wraps VALUE in a fresh ref before
binding it to NAME — equivalent to (let NAME (ref VALUE)). Returns
the underlying value (with the doc attached when callable). The
doc, if any, is attached to the inner callable before the ref is
built, so (show (deref NAME)) surfaces it.

See also: (let NAME VALUE), (ref V), (set! R V)."
        }

        KW_FN => {
            "\
(fn NAME (PARAMS...) BODY)
(fn NAME (PARAMS... . REST) BODY)   ;; variadic via dotted tail
(fn NAME REST BODY)                 ;; variadic via bare ident
(fn NAME PARAMS (doc STR+) BODY)    ;; optional doc slot

Creates a closure capturing the current env (lexical scope), binds
it under NAME, and returns the closure. NAME is bound inside the
body so the function can recurse. PARAMS is a list of identifiers
(use () for zero params). A dotted-tail or bare-ident params form
makes the function variadic. BODY is a single form; use (do ...)
for multi-step bodies.

Errors when called with arity \u{2260} 3, or when NAME or any param is
not an ident.

See also: (defmacro NAME PARAMS BODY), (let NAME VALUE)."
        }

        KW_DEFMACRO => {
            "\
(defmacro NAME (PARAMS...) BODY)
(defmacro NAME PARAMS (doc STR+) BODY)

Defines a macro: a callable whose arguments are passed
unevaluated. The body runs at expansion time and must produce a
form, which is then evaluated in the caller's env. Otherwise
shares fn's parameter shape (positional, dotted-tail, bare-ident
variadic) and optional doc slot.

See also: (fn NAME PARAMS BODY), (quasi DATUM)."
        }

        KW_IF => {
            "\
(if COND THEN ELSE)

Evaluates COND. If truthy evaluates THEN; otherwise evaluates
ELSE. The untaken branch is never evaluated.

Errors when called with arity \u{2260} 3.

See also: (cond ...), (unless COND BODY...)."
        }

        KW_DO => {
            "\
(do FORM...)

Evaluates each form in order, threading the env between them, and
returns the last value (or () if empty). do is not a scope
boundary: later forms see let/fn bindings introduced by earlier
forms, and those bindings leak out to the surrounding env. Pure
sequencing \u{2014} equivalent to splicing the forms into the enclosing
position."
        }

        KW_QUOTE => {
            "\
(quote X)   ;; or 'X

Returns X unevaluated. Identifiers appear as ident values; lists
appear as cons chains.

Errors when called with arity \u{2260} 1.

See also: (quasi DATUM), (eval FORM)."
        }

        KW_QUASIQUOTE => {
            "\
(quasi DATUM)   ;; or `DATUM

Returns DATUM as a literal, except (unquote X) is replaced by the
evaluation of X, and (unquote-splice X) as a list element has X
evaluated and its sequence spliced into the surrounding list.
Recurses into nested lists. No nested-depth tracking \u{2014} unquotes
splice into the nearest enclosing list.

Errors when called with arity \u{2260} 1.

See also: (unquote X), (unquote-splice X), (quote X)."
        }

        KW_UNQUOTE => {
            "\
(unquote X)   ;; or ,X

Only meaningful inside a (quasi ...) form: marks X for evaluation,
with the resulting value substituted into the surrounding
template.

See also: (quasi DATUM), (unquote-splice X)."
        }

        KW_UNQUOTE_SPLICE => {
            "\
(unquote-splice X)   ;; or ,@X

Only meaningful as an element of a list inside a (quasi ...) form:
evaluates X and splices its resulting sequence into the
surrounding list.

Errors when spliced outside a surrounding list.

See also: (quasi DATUM), (unquote X)."
        }

        KW_EVAL => {
            "\
(eval FORM)

Evaluates FORM in the current env and returns its value. Useful
for running forms built from quoted or quasiquoted templates.

See also: (quote X), (quasi DATUM)."
        }

        KW_OPEN => {
            "\
(open PATH)
(open PATH PREFIX)

Reads the rizz source file at PATH, evaluates its top-level forms
in a fresh module env, and merges ALL of the module's top-level
let/fn bindings into the caller's env (including names starting
with _). Returns the value of the loaded module's last form. On a
name collision the loaded module's binding wins.

With an optional PREFIX ident, every merged name is rewritten to
PREFIX.NAME, keeping the module's bindings namespaced.

PATH   — path | ident: if PATH has no extension, .rz is appended.
         Relative paths resolve against the caller's source-file
         directory.
PREFIX — an unevaluated ident; merged names become PREFIX.NAME.

See also: (load PATH), (load-quoted PATH)

Example:
  ;; math.rz
  (fn sin (x) ...)
  (fn cos (x) ...)

  ;; caller.rz
  (open \"math\")          ;; binds `sin`, `cos`
  (open \"math\" math)     ;; binds `math.sin`, `math.cos`
  (math.sin 0)            ;; => 0"
        }

        KW_LOAD => {
            "\
(load PATH)

Reads the rizz source file at PATH, evaluates its top-level forms
in a fresh module env, and returns the module's top-level bindings
as a map keyed by ident. Unlike `open`, nothing is merged into the
caller's env — the bindings are reified as a value.

PATH — path | ident: if PATH has no extension, .rz is appended.
       Relative paths resolve against the caller's source-file
       directory.

See also: (open PATH [PREFIX]), (load-quoted PATH)

Example:
  ;; math.rz
  (fn sin (x) ...)
  (fn cos (x) ...)

  ;; caller.rz
  (let m (load \"math\"))   ;; => { sin : <fn>, cos : <fn> }
  ((get m 'sin) 0)         ;; => 0"
        }

        KW_LOAD_QUOTED => {
            "\
(load-quoted PATH)

Reads the file at PATH and returns its top-level forms as data — a
list of the parsed forms, WITHOUT evaluating them. Useful for
metaprogramming: inspect, transform, or selectively `eval` the
forms a file contains.

PATH — path | ident: if PATH has no extension, .rz is appended.
       Relative paths resolve against the caller's source-file
       directory.

See also: (open PATH [PREFIX]), (load PATH), (eval FORM)

Example:
  ;; mod.rz
  (let answer 42)
  (fn dbl (x) (* x 2))

  ;; caller.rz
  (load-quoted \"mod\")   ;; => ((let answer 42) (fn dbl (x) (* x 2)))"
        }

        KW_DOC => {
            "\
(doc STR+)

Documentation slot for binding forms. Not a standalone special
form \u{2014} only meaningful in the optional doc position of let, let!,
fn, and defmacro. Each argument is evaluated and must produce
either a string or a (recursively flattened) collection of
strings; all collected strings are joined with newlines and stored
on the bound callable.

Errors when given zero arguments.

See also: (show V)."
        }

        _ => return None,
    })
}
