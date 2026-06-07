use crate::{
    Env,
    consts::{
        KW_DEFMACRO, KW_DEFUN, KW_DEFVAR, KW_DEFVAR_REF, KW_DO, KW_DOC, KW_EVAL, KW_IF, KW_OPEN,
        KW_QUASIQUOTE, KW_QUOTE, KW_UNQUOTE, KW_UNQUOTE_SPLICE,
    },
    runtime::{NativeFn, Value},
};
use std::rc::Rc;

pub fn env() -> Env {
    Env::of_builtins(vec![("typeof", typeof_()), ("show", show()), ("id", id())])
}

fn typeof_() -> NativeFn {
    NativeFn::pure("typeof".into(), 1, |args| {
        Ok(Rc::new(Value::Ident(Value::type_name(&args[0]).into())))
    })
    .with_doc(
        "(typeof v): the type name of v as an identifier (e.g. 'int, 'float, 'str, \
         'array, 'map, 'ref, 'cons, 'closure, 'macro, 'native-fn, 'ident, 'unit)."
            .into(),
    )
}

fn id() -> NativeFn {
    NativeFn::pure("id".into(), 1, |args| Ok(args[0].clone()))
        .with_doc("(id v): identity function for `v`. ie returns itself".into())
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
        "(show v): returns the doc string attached to a closure, macro, or native fn at \
         definition time (see the optional (doc \"...\") slot on binding forms). When v is \
         an identifier naming a special form (e.g. 'fn, 'let!, 'defmacro, 'quote), returns \
         the built-in description of that form. When v is any other ident, it is looked up \
         in the env and the bound value's doc is returned. Returns () when no doc is \
         available. Refs are peeled so (show (deref r)) and (show r) behave the same."
            .into(),
    )
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
        KW_DEFVAR => {
            "\
(let NAME VALUE)
(let NAME (doc STR+) VALUE)

Evaluates VALUE, binds it to NAME in the surrounding env, and returns the bound \
value. An optional (doc \"...\") slot documents the binding; the doc is attached \
to callables (closure, macro, native fn) and silently dropped on non-callables. \
Errors: arity \u{2260} 2 (or 3 with a doc slot); NAME not an ident."
        }

        KW_DEFVAR_REF => {
            "\
(let! NAME VALUE)
(let! NAME (doc STR+) VALUE)

Like `let`, but wraps VALUE in a fresh ref before binding it to NAME. Returns the \
underlying value (with the doc attached when callable). Equivalent to \
(let NAME (ref VALUE)). The doc, if any, is attached to the inner callable before \
the ref is built, so (show (deref NAME)) surfaces it."
        }

        KW_DEFUN => {
            "\
(fn NAME (PARAMS...) BODY)
(fn NAME (PARAMS... . REST) BODY)   ;; variadic via dotted tail
(fn NAME REST BODY)                 ;; variadic via bare ident
(fn NAME PARAMS (doc STR+) BODY)    ;; optional doc slot

Creates a closure capturing the current env (lexical scope), binds it under NAME, \
and returns the closure. NAME is bound inside the body so the function can recurse. \
PARAMS is a list of identifiers (use () for zero params). A dotted-tail or bare-ident \
params form makes the function variadic. BODY is a single form; use `do` for \
multi-step bodies. Errors: arity \u{2260} 3; NAME or any param not an ident."
        }

        KW_DEFMACRO => {
            "\
(defmacro NAME (PARAMS...) BODY)
(defmacro NAME PARAMS (doc STR+) BODY)

Defines a macro: a callable whose arguments are passed unevaluated. The body \
runs at expansion time and must produce a form, which is then evaluated in the \
caller's env. Otherwise shares `fn`'s parameter shape (positional, dotted-tail, \
bare-ident variadic) and optional doc slot."
        }

        KW_IF => {
            "\
(if COND THEN ELSE)

Evaluates COND. If truthy evaluates THEN; otherwise evaluates ELSE. The untaken \
branch is never evaluated. Errors: arity \u{2260} 3."
        }

        KW_DO => {
            "\
(do FORM*)

Evaluates each form in order, threading the env between them, and returns the \
last value (or () if empty). `do` is not a scope boundary: later forms see \
let/fn bindings introduced by earlier forms, and those bindings leak out to the \
surrounding env. Pure sequencing \u{2014} equivalent to splicing the forms into \
the enclosing position."
        }

        KW_QUOTE => {
            "\
(quote X)   ;; or 'X

Returns X unevaluated. Identifiers appear as Ident values; lists appear as Cons \
chains. Errors: arity \u{2260} 1."
        }

        KW_QUASIQUOTE => {
            "\
(quasi DATUM)   ;; or `DATUM

Returns DATUM as a literal, except (unquote X) is replaced by the evaluation of \
X, and (unquote-splice X) as a list element has X evaluated and its sequence \
spliced into the surrounding list. Recurses into nested lists. No nested-depth \
tracking \u{2014} unquotes splice into the nearest enclosing list. Errors: \
arity \u{2260} 1."
        }

        KW_UNQUOTE => {
            "\
(unquote X)   ;; or ,X

Only meaningful inside a (quasi ...) form: marks X for evaluation, with the \
resulting value substituted into the surrounding template."
        }

        KW_UNQUOTE_SPLICE => {
            "\
(unquote-splice X)   ;; or ,@X

Only meaningful as an element of a list inside a (quasi ...) form: evaluates X \
and splices its resulting sequence into the surrounding list. Splicing outside \
a surrounding list raises TypeMismatch."
        }

        KW_EVAL => {
            "\
(eval FORM)

Evaluates FORM in the current env and returns its value. Useful for running \
forms built from quoted/quasiquoted templates."
        }

        KW_OPEN => {
            "\
(open PATH)

Loads the rizz source file at PATH, evaluates its top-level forms in a fresh \
module env, and merges the module's top-level let/fn bindings into the caller's \
env. Returns the value of the loaded module's last form. PATH may be a string or \
a bare identifier. If PATH has no extension, `.rz` is appended. Relative paths \
resolve against the caller's source-file directory. Names starting with `_` are \
treated as module-private and are not merged. On a name collision the caller's \
existing binding wins."
        }

        KW_DOC => {
            "\
(doc ARG+)

Documentation slot for binding forms. Not a standalone special form \u{2014} only \
meaningful in the optional doc position of `let`, `let!`, `fn`, and `defmacro`. \
Each ARG is evaluated and must produce either a string or a (recursively \
flattened) collection of strings. All collected strings are joined with \\n and \
stored on the bound callable. A `doc` form with zero arguments raises \
ArityMismatch."
        }

        _ => return None,
    })
}
