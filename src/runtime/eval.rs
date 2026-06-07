//! The tree-walking runtime.
//!
//! [`eval`] threads an [`Env`] through evaluation: it takes a form and an
//! environment and returns the resulting value together with a (possibly
//! extended) environment, so top-level `let`/`fn` definitions are visible to
//! later forms. When a list's head is one of the keywords below it is handled
//! as a special form; otherwise the list is a function application.

use crate::{
    consts::{
        FILE_EXTENSION, KW_DEFMACRO, KW_DEFUN, KW_DEFVAR, KW_DEFVAR_REF, KW_DO, KW_DOC, KW_EVAL,
        KW_IF, KW_OPEN, KW_QUASIQUOTE, KW_QUOTE, KW_UNQUOTE, KW_UNQUOTE_SPLICE,
    },
    runtime::{Closure, Env, RuntimeError, Value},
};
use anyhow::anyhow;
use im::{HashMap, Vector};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Evaluates `form` in environment `ctx`, returning the value and the resulting
/// environment.
///
/// Atoms self-evaluate; identifiers resolve via the environment; a `Cons`
/// whose head is a special-form keyword is dispatched accordingly, and any
/// other `Cons` is a function application — the head is evaluated to a
/// callable, which is then applied as a native function or closure.
pub fn eval(form: Rc<Value>, ctx: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    match &*form {
        Value::Int(_)
        | Value::Unit
        | Value::Str(_)
        | Value::Float(_)
        | Value::NativeFn(_)
        | Value::Macro(_)
        | Value::Ref(_) => Ok((form.clone(), ctx.clone())),
        Value::Ident(ident) => {
            let form = ctx
                .get(ident)
                .ok_or(RuntimeError::UnknownIdent(ident.clone()))?;

            Ok((form.clone(), ctx.clone()))
        }
        Value::Cons { head, tail } => {
            if let Value::Ident(ident) = &**head {
                match ident.as_ref() {
                    KW_DEFVAR => return eval_defvar(tail, ctx),
                    KW_DEFVAR_REF => return eval_defvar_ref(tail, ctx),
                    KW_QUOTE => return eval_quote(tail, ctx),
                    KW_QUASIQUOTE => return eval_quasiquote(tail, ctx),
                    KW_DEFUN => return eval_defun(tail, ctx),
                    KW_DEFMACRO => return eval_defmacro(tail, ctx),
                    KW_IF => return eval_if(tail, ctx),
                    KW_DO => return eval_do(tail, ctx),
                    KW_EVAL => return eval(tail.clone(), ctx),
                    KW_OPEN => return eval_open(tail, ctx),
                    _ => {}
                }
            }
            let (callable, ctx) = eval(head.clone(), ctx)?;
            let callable = deref_callable(callable);
            match &*callable {
                Value::NativeFn(native) => {
                    // For Pure/WithEnv/Macro, `call` returns the caller's ctx
                    // unchanged — bindings introduced while evaluating
                    // arguments thread between siblings but must not leak past
                    // the call. The Impure arm returns an env extended by `f`,
                    // which we propagate.
                    native.call(tail, &ctx)
                }
                Value::Closure(closure) => {
                    let (args, _) = eval_and_collect(tail, &ctx)?;
                    let (v, _) = eval_closure(&args, closure)?;
                    Ok((v, ctx))
                }
                Value::Macro(macro_) => {
                    // Bind unevaluated arguments to the macro's params,
                    // evaluate its body to produce an expansion form, then
                    // evaluate the expansion in the caller's environment.
                    let args: Vec<_> = Value::iter(tail).collect();
                    let (expansion, _) = expand_macro(&args, macro_)?;
                    let (v, _) = eval(expansion, &ctx)?;
                    Ok((v, ctx))
                }
                Value::Int(_)
                | Value::Ref(_)
                | Value::Unit
                | Value::Str(_)
                | Value::Float(_)
                | Value::Cons { .. }
                | Value::Array(_)
                | Value::Map(_)
                | Value::Ident(_) => Err(RuntimeError::NotCallable { value: callable }),
            }
        }
        Value::Closure(_) => Ok((form.clone(), ctx.clone())),
        Value::Array(xs) => {
            // Each element is evaluated in the surrounding env independently;
            // bindings do not thread between siblings and do not leak outward.
            let mut out = Vector::new();
            for x in xs.iter() {
                let (v, _) = eval(x.clone(), ctx)?;
                out.push_back(v);
            }
            Ok((Rc::new(Value::Array(out)), ctx.clone()))
        }
        Value::Map(m) => {
            let mut out = HashMap::new();
            for (k, v) in m.iter() {
                let (kv, _) = eval(k.clone(), ctx)?;
                let (vv, _) = eval(v.clone(), ctx)?;
                out.insert(kv, vv);
            }
            Ok((Rc::new(Value::Map(out)), ctx.clone()))
        }
    }
}

/// Applies a callable to already-evaluated `args`. Dispatches closures through
/// [`eval_closure`] and native fns through [`NativeFn::apply`](crate::runtime::native::NativeFn). The caller's
/// `ctx` is threaded to native fns but the returned env is discarded (a call
/// must not leak callee bindings), so only the resulting value is returned.
pub fn apply(
    callable: &Rc<Value>,
    args: &[Rc<Value>],
    ctx: &Env,
) -> Result<Rc<Value>, RuntimeError> {
    let callable = deref_callable(callable.clone());
    match &*callable {
        Value::Closure(c) => {
            let (v, _) = eval_closure(args, c)?;
            Ok(v)
        }
        Value::NativeFn(n) => {
            let (v, _) = n.apply(args, ctx)?;
            Ok(v)
        }
        _ => Err(RuntimeError::NotCallable {
            value: callable.clone(),
        }),
    }
}

/// Peels `Value::Ref` layers off `v` as long as the cell holds something the
/// runtime can dispatch (a native fn, closure, macro, or another such ref).
/// Stops at the first non-dispatchable value and returns it unchanged, so the
/// usual `NotCallable` path still reports a sensible value.
fn deref_callable(v: Rc<Value>) -> Rc<Value> {
    fn is_dispatchable(v: &Value) -> bool {
        match v {
            Value::NativeFn(_) | Value::Closure(_) | Value::Macro(_) => true,
            Value::Ref(cell) => is_dispatchable(&cell.borrow()),
            _ => false,
        }
    }
    let mut cur = v;
    while let Value::Ref(cell) = &*cur.clone() {
        let inner = cell.borrow().clone();
        if !is_dispatchable(&inner) {
            break;
        }
        cur = Rc::new(inner);
    }
    cur
}

pub fn eval_and_collect(
    tail: &Rc<Value>,
    ctx: &Env,
) -> Result<(Vec<Rc<Value>>, Env), RuntimeError> {
    let mut args = Vec::new();
    let mut ctx = ctx.clone();
    for arg in Value::iter(tail) {
        let (v, env) = eval(arg, &ctx)?;
        args.push(v);
        ctx = env;
    }
    Ok((args, ctx))
}

/// Applies `closure` to `args`: checks arity, binds the closure to its own
/// name (so the body can recurse) and each parameter to its argument in the
/// captured environment, then evaluates the body. For variadic closures
/// (`closure.rest = Some(_)`) the trailing arguments past `params.len()` are
/// bundled into a cons list and bound under the rest name.
fn eval_closure(
    args: &[Rc<Value>],
    closure: &Rc<Closure>,
) -> Result<(Rc<Value>, Env), RuntimeError> {
    bind_and_eval(
        args,
        &closure.name,
        &closure.params,
        closure.rest.as_ref(),
        &closure.body,
        &closure.env,
        Rc::new(Value::Closure(closure.clone())),
    )
}

/// Expands `macro_` with `args` (unevaluated): checks arity, self-binds the
/// macro under its own name so the body can recursively expand to a call to
/// itself, binds each parameter to its (unevaluated) argument in the captured
/// environment, then evaluates the body to produce the expansion form.
/// Variadic macros bundle the trailing unevaluated forms as a cons list.
fn expand_macro(
    args: &[Rc<Value>],
    macro_: &Rc<Closure>,
) -> Result<(Rc<Value>, Env), RuntimeError> {
    bind_and_eval(
        args,
        &macro_.name,
        &macro_.params,
        macro_.rest.as_ref(),
        &macro_.body,
        &macro_.env,
        Rc::new(Value::Macro(macro_.clone())),
    )
}

/// Shared body for closure invocation and macro expansion: checks arity, binds
/// the self-reference plus the positional and rest params, then evaluates the
/// body in the resulting env.
fn bind_and_eval(
    args: &[Rc<Value>],
    name: &Rc<str>,
    params: &[Rc<str>],
    rest: Option<&Rc<str>>,
    body: &Rc<Value>,
    captured: &Env,
    self_value: Rc<Value>,
) -> Result<(Rc<Value>, Env), RuntimeError> {
    match rest {
        None if params.len() != args.len() => {
            return Err(RuntimeError::ArityMismatch {
                name: name.clone(),
                expected: params.len(),
                got: args.len(),
            });
        }
        Some(_) if args.len() < params.len() => {
            return Err(RuntimeError::ArityMismatch {
                name: name.clone(),
                expected: params.len(),
                got: args.len(),
            });
        }
        _ => {}
    }

    let mut call_env = captured.clone().update(name.clone(), self_value);
    for (ident, arg) in params.iter().zip(args) {
        call_env = call_env.update(ident.clone(), arg.clone());
    }
    if let Some(rest_name) = rest {
        let tail = args[params.len()..].iter().cloned();
        let rest_list = cons_list_from(tail);
        call_env = call_env.update(rest_name.clone(), rest_list);
    }
    eval(body.clone(), &call_env)
}

/// Builds a cons list from an iterator of values, terminated by `Unit`.
fn cons_list_from<I>(items: I) -> Rc<Value>
where
    I: IntoIterator<Item = Rc<Value>>,
    I::IntoIter: DoubleEndedIterator,
{
    items.into_iter().rfold(Rc::new(Value::Unit), |tail, head| {
        Rc::new(Value::Cons { head, tail })
    })
}

/// `(fn name (params...) body)`: builds a [`Closure`] capturing `env`, binds it
/// under `name`, and returns the closure along with the extended environment.
///
/// The param list accepts three shapes:
/// - `(a b c)`: fixed arity.
/// - `(a b . rest)`: dotted tail — `a` and `b` are required positional params;
///   any remaining arguments are bundled into a cons list bound to `rest`.
/// - bare ident `args`: shorthand for `(. args)` — every argument is bundled
///   into the rest list, no positional params.
///
/// An optional `(doc "...")` form may sit between the params and the body
/// (`(fn NAME PARAMS (doc "...") BODY)`); the doc is attached to the closure
/// and retrievable via the `show` builtin.
///
/// `NAME` is optional: `(fn PARAMS BODY)` and `(fn PARAMS (doc ...) BODY)` build
/// an anonymous closure that is not bound into the surrounding env. Anonymous
/// closures cannot self-reference by name, so they cannot recurse.
fn eval_defun(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    let (name, params_form, doc, body) = split_fn_form(&items, env)?;
    let (params, rest) = parse_param_list(KW_DEFUN, &params_form)?;

    let closure = Rc::new(Value::Closure(Rc::new(Closure {
        name: name.clone().unwrap_or_else(|| "".into()),
        params,
        rest,
        body,
        env: env.clone(),
        doc,
    })));
    let env = match name {
        Some(n) => env.clone().update(n, closure.clone()),
        None => env.clone(),
    };
    Ok((closure, env))
}

/// The tail of a `fn` form: `(name, params, doc, body)`. Accepts:
/// - `(PARAMS BODY)` — anonymous.
/// - `(PARAMS (doc ...) BODY)` — anonymous with doc.
/// - `(NAME PARAMS BODY)` — named.
/// - `(NAME PARAMS (doc ...) BODY)` — named with doc.
type CallableTail = (Option<Rc<str>>, Rc<Value>, Option<Rc<str>>, Rc<Value>);

/// The 3-element shape disambiguates on whether the middle item is a `(doc ...)`
/// form: if yes, it's the anonymous-with-doc shape; otherwise it's the named
/// shape. This means a 3-element call with a non-ident first slot still falls
/// into the named path and surfaces a "name must be ident" error.
fn split_fn_form(items: &[Rc<Value>], env: &Env) -> Result<CallableTail, RuntimeError> {
    match items.len() {
        2 => Ok((None, items[0].clone(), None, items[1].clone())),
        3 if is_doc_form(&items[1]) => {
            let doc = parse_doc_form(KW_DEFUN, &items[1], env)?;
            Ok((None, items[0].clone(), Some(doc), items[2].clone()))
        }
        3 => {
            let name = expect_name(&items[0])?;
            Ok((Some(name), items[1].clone(), None, items[2].clone()))
        }
        4 if is_doc_form(&items[2]) => {
            let name = expect_name(&items[0])?;
            let doc = parse_doc_form(KW_DEFUN, &items[2], env)?;
            Ok((Some(name), items[1].clone(), Some(doc), items[3].clone()))
        }
        _ => Err(RuntimeError::ArityMismatch {
            name: KW_DEFUN.into(),
            expected: 3,
            got: items.len(),
        }),
    }
}

fn expect_name(form: &Rc<Value>) -> Result<Rc<str>, RuntimeError> {
    match &**form {
        Value::Ident(name) => Ok(name.clone()),
        other => Err(RuntimeError::TypeMismatch {
            name: KW_DEFUN.into(),
            expected: "ident".into(),
            got: Value::type_name(other).into(),
        }),
    }
}

/// Splits the tail of a `fn` / `defmacro` form into `(name, params, doc, body)`.
/// Accepts either the 3-element shape `(NAME PARAMS BODY)` or the 4-element
/// shape `(NAME PARAMS (doc ...) BODY)` — the 4-element shape is only chosen
/// when the third item is a `doc` form; otherwise it's an arity error so that
/// `(fn f () x extra)` still surfaces a "too many args" message.
fn split_callable_form(
    form: &'static str,
    items: &[Rc<Value>],
    env: &Env,
) -> Result<CallableTail, RuntimeError> {
    let (name_form, params_form, doc, body) = match items.len() {
        3 => (&items[0], &items[1], None, items[2].clone()),
        4 if is_doc_form(&items[2]) => {
            let doc = parse_doc_form(form, &items[2], env)?;
            (&items[0], &items[1], Some(doc), items[3].clone())
        }
        _ => {
            return Err(RuntimeError::ArityMismatch {
                name: form.into(),
                expected: 3,
                got: items.len(),
            });
        }
    };
    let Value::Ident(name) = &**name_form else {
        return Err(RuntimeError::TypeMismatch {
            name: form.into(),
            expected: "ident".into(),
            got: Value::type_name(name_form).into(),
        });
    };
    Ok((Some(name.clone()), params_form.clone(), doc, body))
}

/// Whether `v` is a `(doc ...)` list — used to decide whether an extra slot in
/// a binding form is a doc form or an arity error.
fn is_doc_form(v: &Rc<Value>) -> bool {
    matches!(&**v, Value::Cons { head, .. } if matches!(&**head, Value::Ident(s) if s.as_ref() == KW_DOC))
}

/// Reads a `(doc ARG ARG ...)` form. Each argument is evaluated; the result
/// must be a string or a collection (list/array) of strings, which is
/// flattened. All collected strings are joined with `\n` to produce the
/// stored doc. Errors if the form isn't a `doc` list, has no arguments, or
/// any evaluated argument isn't a string or collection of strings.
fn parse_doc_form(
    form: &'static str,
    value: &Rc<Value>,
    env: &Env,
) -> Result<Rc<str>, RuntimeError> {
    let Value::Cons { head, tail } = &**value else {
        return Err(RuntimeError::TypeMismatch {
            name: form.into(),
            expected: "(doc ...) or body".into(),
            got: Value::type_name(value).into(),
        });
    };
    let head_is_doc = matches!(&**head, Value::Ident(s) if s.as_ref() == KW_DOC);
    if !head_is_doc {
        return Err(RuntimeError::TypeMismatch {
            name: form.into(),
            expected: "(doc ...)".into(),
            got: "list".into(),
        });
    }
    let args: Vec<_> = Value::iter(tail).collect();
    if args.is_empty() {
        return Err(RuntimeError::ArityMismatch {
            name: KW_DOC.into(),
            expected: 1,
            got: 0,
        });
    }
    let mut parts: Vec<String> = Vec::new();
    for arg in args {
        let (val, _) = eval(arg, env)?;
        collect_doc_strings(&val, &mut parts)?;
    }
    Ok(parts.join("\n").into())
}

/// Recursively flattens a doc argument into `parts`. Accepts strings,
/// arrays, and cons lists (including the empty list `Unit`). Any other
/// value kind — or a collection containing a non-string — is a type error.
fn collect_doc_strings(v: &Rc<Value>, parts: &mut Vec<String>) -> Result<(), RuntimeError> {
    match &**v {
        Value::Str(s) => parts.push(s.to_string()),
        Value::Array(xs) => {
            for x in xs {
                collect_doc_strings(x, parts)?;
            }
        }
        Value::Unit | Value::Cons { .. } => {
            for item in Value::iter(v) {
                collect_doc_strings(&item, parts)?;
            }
        }
        _ => {
            return Err(RuntimeError::TypeMismatch {
                name: KW_DOC.into(),
                expected: "str or collection of str".into(),
                got: Value::type_name(v).into(),
            });
        }
    }
    Ok(())
}

/// Applies `doc` to `value` if it carries a doc slot. Closures and macros are
/// rebuilt with the doc field set; native fns are wrapped via [`NativeFn::with_doc`].
/// Other value kinds silently drop the doc — see [`eval_defvar`].
fn attach_doc(value: Rc<Value>, doc: Option<Rc<str>>) -> Rc<Value> {
    let Some(doc) = doc else { return value };
    match &*value {
        Value::Closure(c) => {
            let mut new = (**c).clone();
            new.doc = Some(doc);
            Rc::new(Value::Closure(Rc::new(new)))
        }
        Value::Macro(c) => {
            let mut new = (**c).clone();
            new.doc = Some(doc);
            Rc::new(Value::Macro(Rc::new(new)))
        }
        Value::NativeFn(n) => Rc::new(Value::NativeFn(Rc::new((**n).clone().with_doc(doc)))),
        _ => value,
    }
}

type ParamList = (Vec<Rc<str>>, Option<Rc<str>>);
/// Walks the param-list form for `fn`/`defmacro`. Accepts a cons chain of
/// idents (`(a b c)`), a dotted chain whose final tail is an ident
/// (`(a b . rest)`), or a single bare ident (fully variadic).
fn parse_param_list(
    form: &'static str,
    params_form: &Rc<Value>,
) -> Result<ParamList, RuntimeError> {
    if let Value::Ident(name) = &**params_form {
        return Ok((Vec::new(), Some(name.clone())));
    }

    let mut params = Vec::new();
    let mut cur = params_form.clone();
    loop {
        match &*cur.clone() {
            Value::Unit => return Ok((params, None)),
            Value::Cons { head, tail } => {
                let Value::Ident(p) = &**head else {
                    return Err(RuntimeError::TypeMismatch {
                        name: form.into(),
                        expected: "ident".into(),
                        got: Value::type_name(head).into(),
                    });
                };
                params.push(p.clone());
                cur = tail.clone();
            }
            Value::Ident(rest_name) => return Ok((params, Some(rest_name.clone()))),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    name: form.into(),
                    expected: "ident".into(),
                    got: Value::type_name(other).into(),
                });
            }
        }
    }
}

/// `(defmacro name (params...) body)`: builds a [`Closure`] capturing `env`,
/// wraps it as a [`Value::Macro`], binds it under `name`, and returns the macro
/// along with the extended environment. At a call site, a macro receives its
/// arguments **unevaluated** and its body's result is then evaluated in the
/// caller's env (see the `Value::Macro` arm of [`eval`]).
///
/// An optional `(doc "...")` form may sit between the params and the body
/// (`(defmacro NAME PARAMS (doc "...") BODY)`); see [`eval_defun`].
fn eval_defmacro(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    let (name, params_form, doc, body) = split_callable_form(KW_DEFMACRO, &items, env)?;
    let (params, rest) = parse_param_list(KW_DEFMACRO, &params_form)?;
    let name = name.expect("`split_callable_form` ensures we have name");
    let mac = Rc::new(Value::Macro(Rc::new(Closure {
        name: Rc::clone(&name),
        params,
        rest,
        body,
        env: env.clone(),
        doc,
    })));
    let env = env.clone().update(Rc::clone(&name), mac.clone());
    Ok((mac, env))
}

/// `(let name value)`: evaluates `value`, binds it to `name`, and returns the
/// value with the extended environment.
///
/// An optional `(doc "...")` form may sit between the name and the value
/// (`(let NAME (doc "...") VALUE)`). If the resulting value is a callable
/// (closure, macro, or native fn) the doc is attached to it; otherwise the
/// doc is silently dropped — non-callable values have no doc slot.
fn eval_defvar(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    let (name, doc, value_form) = split_var_form(KW_DEFVAR, &items, env)?;
    let (val, env) = eval(value_form, env)?;
    let val = attach_doc(val, doc);
    let env = env.update(name, val.clone());
    Ok((val, env))
}

type DefvarTail = (Rc<str>, Option<Rc<str>>, Rc<Value>);
/// Splits the tail of a `let` / `let!` form into `(name, doc, value)`. Accepts
/// either `(NAME VALUE)` or `(NAME (doc ...) VALUE)` — the 3-element shape is
/// only chosen when the middle item is a `doc` form so that `(let x 1 2)` still
/// surfaces as an arity error rather than "expected doc form".
fn split_var_form(
    form: &'static str,
    items: &[Rc<Value>],
    env: &Env,
) -> Result<DefvarTail, RuntimeError> {
    let (name_form, doc, value_form) = match items.len() {
        2 => (&items[0], None, items[1].clone()),
        3 if is_doc_form(&items[1]) => {
            let doc = parse_doc_form(form, &items[1], env)?;
            (&items[0], Some(doc), items[2].clone())
        }
        _ => {
            return Err(RuntimeError::ArityMismatch {
                name: form.into(),
                expected: 2,
                got: items.len(),
            });
        }
    };
    let Value::Ident(name) = &**name_form else {
        return Err(RuntimeError::TypeMismatch {
            name: form.into(),
            expected: "ident".into(),
            got: Value::type_name(name_form).into(),
        });
    };
    Ok((name.clone(), doc, value_form))
}

/// `(open path)`: loads and evaluates the file at `path` (extension `.rz` is
/// appended if absent). Relative paths are resolved against the current source
/// file's directory (see [`Env::base_dir`]), falling back to the process CWD.
/// The loaded module's top-level bindings are merged into the caller's env
/// — minus any whose names start with `_` (a convention for private items) —
/// and the value of the loaded module's last form is returned. `open` is a
/// special form rather than a native fn because native calls cannot leak
/// bindings into the caller; module loading is precisely the operation that
/// must.
fn eval_open(tail: &Rc<Value>, ctx: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 1 {
        return Err(RuntimeError::ArityMismatch {
            name: KW_OPEN.into(),
            expected: 1,
            got: items.len(),
        });
    }
    let (arg, ctx) = eval(items[0].clone(), ctx)?;
    let mut path = arg
        .as_str_or_ident()
        .ok_or_else(|| RuntimeError::type_mismatch(KW_OPEN, "str", &arg))
        .map(|s| PathBuf::from(s.as_ref()))?;
    if path.extension().is_none() {
        path.set_extension(FILE_EXTENSION);
    }
    if path.is_relative()
        && let Some(base) = ctx.base_dir()
    {
        path = base.join(path);
    }
    let child_base = path.parent().map(PathBuf::from);
    let f = std::fs::File::open(&path)?;
    let child_env = match ctx.base_env() {
        Some(base) => (**base)
            .clone()
            .with_base_dir(child_base)
            .with_base_env(base.clone()),
        None => Env::new().with_base_dir(child_base),
    };
    let (v, loaded) =
        crate::parse_and_run_with_env(f, &child_env).map_err(|e| anyhow!(e.to_string()))?;
    let env = ctx.union(loaded.filter(|(k, _)| !k.starts_with('_')));
    Ok((v, env))
}

/// `(let! name value)`: evaluates `value`, wraps it in a ref, binds it to
/// `name`, and returns the value with the extended environment.
///
/// An optional `(doc "...")` slot is accepted in the same position as for
/// [`let`](eval_defvar); if the underlying value is callable the doc is
/// attached to it before the ref is built (so `(show (deref name))` works).
fn eval_defvar_ref(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    let (name, doc, value_form) = split_var_form(KW_DEFVAR_REF, &items, env)?;
    let (val, env) = eval(value_form, env)?;
    let val = attach_doc(val, doc);
    let env = env.update(
        name,
        Value::Ref(Rc::new(RefCell::new(val.as_ref().clone()))).into(),
    );
    Ok((val, env))
}

/// `(do f1 f2 ... fn)`: pure sequencing. Evaluates each form in order,
/// threading the env so a `let`/`fn` introduced by an earlier form is visible
/// to later ones, and returns the last form's value (Unit for an empty `(do)`)
/// along with the threaded env. `do` is not a scope boundary: bindings
/// introduced inside it leak to the surrounding env, just as with top-level
/// sequencing.
fn eval_do(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let mut inner = env.clone();
    let mut last = Rc::new(Value::Unit);
    for form in Value::iter(tail) {
        let (v, e) = eval(form, &inner)?;
        last = v;
        inner = e;
    }
    Ok((last, inner))
}

/// `(if cond then else)`: evaluates `cond`, then evaluates only the matching
/// branch so the untaken branch never runs.
fn eval_if(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 3 {
        return Err(RuntimeError::ArityMismatch {
            name: KW_IF.into(),
            expected: 3,
            got: items.len(),
        });
    }

    let (cond, _) = eval(items[0].clone(), env)?;
    let (v, _) = if cond.is_truthy() {
        eval(items[1].clone(), env)?
    } else {
        eval(items[2].clone(), env)?
    };
    Ok((v, env.clone()))
}

/// `(quote x)`: returns `x` unevaluated.
fn eval_quote(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 1 {
        return Err(RuntimeError::ArityMismatch {
            name: KW_QUOTE.into(),
            expected: 1,
            got: items.len(),
        });
    }

    Ok((items[0].clone(), env.clone()))
}

/// `(quasi x)`: returns `x` as a literal, except that `unquote` subforms are
/// evaluated and `unquote-splice` elements are spliced in (see [`quasi`]).
fn eval_quasiquote(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 1 {
        return Err(RuntimeError::ArityMismatch {
            name: KW_QUASIQUOTE.into(),
            expected: 1,
            got: items.len(),
        });
    }

    Ok((quasi(&items[0], env)?, env.clone()))
}

/// Rebuild `datum` as a literal structure, evaluating any `(unquote X)` subform
/// and splicing the elements of any `(unquote-splicing X)` element.
fn quasi(datum: &Rc<Value>, env: &Env) -> Result<Rc<Value>, RuntimeError> {
    if let Some(tail) = tagged(datum, KW_UNQUOTE) {
        return Ok(eval(unquote_operand(KW_UNQUOTE, tail)?, env)?.0);
    }
    if tagged(datum, KW_UNQUOTE_SPLICE).is_some() {
        return Err(RuntimeError::TypeMismatch {
            name: KW_UNQUOTE_SPLICE.into(),
            expected: "list context".into(),
            got: KW_QUASIQUOTE.into(),
        });
    }
    match &**datum {
        Value::Cons { .. } => {
            let mut out: Vec<Rc<Value>> = Vec::new();
            for elem in Value::iter(datum) {
                if let Some(tail) = tagged(&elem, KW_UNQUOTE_SPLICE) {
                    let (spliced, _) = eval(unquote_operand(KW_UNQUOTE_SPLICE, tail)?, env)?;
                    out.extend(Value::iter(&spliced));
                } else {
                    out.push(quasi(&elem, env)?);
                }
            }
            Ok(out
                .into_iter()
                .rev()
                .fold(Rc::new(Value::Unit), |tail, head| {
                    Rc::new(Value::Cons { head, tail })
                }))
        }
        Value::Array(xs) => {
            let mut out = Vector::new();
            for x in xs.iter() {
                if let Some(tail) = tagged(x, KW_UNQUOTE_SPLICE) {
                    let (spliced, _) = eval(unquote_operand(KW_UNQUOTE_SPLICE, tail)?, env)?;
                    for e in Value::iter(&spliced) {
                        out.push_back(e);
                    }
                } else {
                    out.push_back(quasi(x, env)?);
                }
            }
            Ok(Rc::new(Value::Array(out)))
        }
        Value::Map(m) => {
            let mut out = HashMap::new();
            for (k, v) in m.iter() {
                out.insert(quasi(k, env)?, quasi(v, env)?);
            }
            Ok(Rc::new(Value::Map(out)))
        }
        _ => Ok(datum.clone()),
    }
}

/// If `v` is a list `(name . rest)`, returns its tail; otherwise `None`.
fn tagged<'a>(v: &'a Value, name: &str) -> Option<&'a Rc<Value>> {
    match v {
        Value::Cons { head, tail } => match &**head {
            Value::Ident(s) if s.as_ref() == name => Some(tail),
            _ => None,
        },
        _ => None,
    }
}

/// Extracts the single operand `x` from an `(unquote x)` / `(unquote-splice x)`
/// tail, erroring if the form does not have exactly one argument.
fn unquote_operand(name: &'static str, tail: &Rc<Value>) -> Result<Rc<Value>, RuntimeError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 1 {
        return Err(RuntimeError::ArityMismatch {
            name: name.into(),
            expected: 1,
            got: items.len(),
        });
    }
    Ok(items[0].clone())
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use super::*;
    use crate::runtime::NativeFn;

    // ----- helpers -----

    fn int(n: i64) -> Rc<Value> {
        Rc::new(Value::Int(n))
    }
    fn float(f: f64) -> Rc<Value> {
        Rc::new(Value::Float(f.into()))
    }
    fn string(s: &str) -> Rc<Value> {
        Rc::new(Value::Str(s.into()))
    }
    fn ident(s: &str) -> Rc<Value> {
        Rc::new(Value::Ident(s.into()))
    }
    fn unit() -> Rc<Value> {
        Rc::new(Value::Unit)
    }

    /// Build a cons-list `Value` from its elements (mirrors `parser::list`).
    fn list(elems: Vec<Rc<Value>>) -> Rc<Value> {
        elems
            .into_iter()
            .rev()
            .fold(unit(), |tail, head| Rc::new(Value::Cons { head, tail }))
    }

    /// A two-arg integer-add native fn used to drive the application arms of
    /// `eval`. Arity is enforced by `NativeFn::call`.
    fn add_builtin() -> Rc<Value> {
        Rc::new(Value::NativeFn(Rc::new(NativeFn::pure(
            "plus".into(),
            2,
            |args: &[Rc<Value>]| -> Result<Rc<Value>, RuntimeError> {
                let a = args[0].as_int().expect("int");
                let b = args[1].as_int().expect("int");
                Ok(int(a + b))
            },
        ))))
    }

    fn eval_ok(form: Rc<Value>, env: &Env) -> (Rc<Value>, Env) {
        eval(form, env).expect("expected successful eval")
    }
    fn eval_err(form: Rc<Value>, env: &Env) -> RuntimeError {
        eval(form, env).expect_err("expected eval error")
    }
    fn lookup(env: &Env, name: &str) -> Rc<Value> {
        let key: Rc<str> = name.into();
        env.get(&key).expect("binding should exist").clone()
    }

    // ----- self-evaluating literals -----

    #[test]
    fn int_self_evaluates() {
        let (v, _) = eval_ok(int(42), &Env::new());
        assert_eq!(*v, Value::Int(42));
    }

    #[test]
    fn float_self_evaluates() {
        let (v, _) = eval_ok(float(3.5), &Env::new());
        assert_eq!(*v, Value::Float(3.5.into()));
    }

    #[test]
    fn str_self_evaluates() {
        let (v, _) = eval_ok(string("hi"), &Env::new());
        assert_eq!(*v, Value::Str("hi".into()));
    }

    #[test]
    fn unit_self_evaluates() {
        let (v, _) = eval_ok(unit(), &Env::new());
        assert_eq!(*v, Value::Unit);
    }

    #[test]
    fn builtin_self_evaluates() {
        let (v, _) = eval_ok(add_builtin(), &Env::new());
        assert!(matches!(&*v, Value::NativeFn(_)));
    }

    #[test]
    fn self_eval_returns_env_unchanged() {
        let env = Env::new().update("x".into(), int(1));
        let (_, out) = eval_ok(int(9), &env);
        assert_eq!(out, env);
    }

    // ----- identifier lookup -----

    #[test]
    fn bound_ident_resolves_to_its_value() {
        let env = Env::new().update("x".into(), int(42));
        let (v, _) = eval_ok(ident("x"), &env);
        assert_eq!(*v, Value::Int(42));
    }

    #[test]
    fn unbound_ident_errors() {
        let err = eval_err(ident("nope"), &Env::new());
        assert!(matches!(err, RuntimeError::UnknownIdent(s) if &*s == "nope"));
    }

    #[test]
    fn ident_lookup_returns_env_unchanged() {
        let env = Env::new().update("x".into(), int(7));
        let (_, out) = eval_ok(ident("x"), &env);
        assert_eq!(out, env);
    }

    // ----- let special form -----

    #[test]
    fn let_returns_the_bound_value() {
        let form = list(vec![ident("let"), ident("x"), int(5)]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn let_binds_name_in_returned_env() {
        let form = list(vec![ident("let"), ident("x"), int(5)]);
        let (_, env) = eval_ok(form, &Env::new());
        assert_eq!(*lookup(&env, "x"), Value::Int(5));
    }

    #[test]
    fn let_evaluates_its_value_expression() {
        // (let y x) with x already bound to 5 -> y bound to 5, returns 5.
        let env = Env::new().update("x".into(), int(5));
        let form = list(vec![ident("let"), ident("y"), ident("x")]);
        let (v, env) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(5));
        assert_eq!(*lookup(&env, "y"), Value::Int(5));
    }

    #[test]
    fn let_too_few_args_errors() {
        let form = list(vec![ident("let"), ident("x")]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::ArityMismatch {
                expected: 2,
                got: 1,
                ..
            }
        ));
    }

    #[test]
    fn let_too_many_args_errors() {
        let form = list(vec![ident("let"), ident("x"), int(1), int(2)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::ArityMismatch {
                expected: 2,
                got: 3,
                ..
            }
        ));
    }

    #[test]
    fn let_non_ident_name_errors() {
        let form = list(vec![ident("let"), int(5), int(10)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::TypeMismatch { expected, got, .. }
                if &*expected == "ident" && &*got == "int"
        ));
    }

    #[test]
    fn let_propagates_value_eval_error() {
        // (let x undefined) -> evaluating the value expr fails.
        let form = list(vec![ident("let"), ident("x"), ident("undefined")]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(err, RuntimeError::UnknownIdent(s) if &*s == "undefined"));
    }

    // ----- cons head dispatch -----

    #[test]
    fn unbound_ident_in_head_position_errors() {
        // A non-special-form head ident falls through to application, where it
        // is looked up like any other ident and fails if unbound.
        let form = list(vec![ident("frobnicate"), int(1)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(err, RuntimeError::UnknownIdent(s) if &*s == "frobnicate"));
    }

    // ----- function application -----

    #[test]
    fn builtin_applied_by_name() {
        // (plus 1 2) -> the head ident resolves to the builtin, which is applied.
        let env = Env::new().update("plus".into(), add_builtin());
        let form = list(vec![ident("plus"), int(1), int(2)]);
        let (v, _) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(3));
    }

    #[test]
    fn closure_applied_by_name() {
        // (id 5) where id is the identity closure -> 5.
        let id = Rc::new(Value::Closure(Rc::new(Closure {
            name: "id".into(),
            params: vec!["x".into()],
            rest: None,
            body: ident("x"),
            env: Env::new(),
            doc: None,
        })));
        let env = Env::new().update("id".into(), id);
        let form = list(vec![ident("id"), int(5)]);
        let (v, _) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn builtin_applied_when_head_expression_yields_it() {
        // The head need not be a bare ident: any expression evaluating to a
        // callable works. `(let f plus)` evaluates to the `plus` builtin.
        let env = Env::new().update("plus".into(), add_builtin());
        let head = list(vec![ident(KW_DEFVAR), ident("f"), ident("plus")]);
        let form = list(vec![head, int(1), int(2)]);
        let (v, _) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(3));
    }

    #[test]
    fn application_evaluates_its_arguments() {
        // (plus ( x 2 )) with x bound to 40 -> 42; arguments are evaluated first.
        let env = Env::new()
            .update("plus".into(), add_builtin())
            .update("x".into(), int(40));
        let form = list(vec![ident("plus"), ident("x"), int(2)]);
        let (v, _) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(42));
    }

    #[test]
    fn builtin_arity_error_propagates_through_application() {
        let env = Env::new().update("plus".into(), add_builtin());
        let form = list(vec![ident("plus"), int(1)]);
        let err = eval_err(form, &env);
        assert!(matches!(
            err,
            RuntimeError::ArityMismatch {
                expected: 2,
                got: 1,
                ..
            }
        ));
    }

    // ----- closures -----

    #[test]
    fn closure_value_self_evaluates() {
        // A bare closure value is a value, not a call: evaluating it returns
        // the closure itself rather than invoking it.
        let clo = Rc::new(Value::Closure(Rc::new(Closure {
            name: "".into(),
            params: vec![],
            rest: None,
            body: int(7),
            env: Env::new(),
            doc: None,
        })));
        let (v, _) = eval_ok(clo.clone(), &Env::new());
        assert!(matches!(&*v, Value::Closure(_)));
        assert_eq!(v, clo);
    }

    #[test]
    fn closure_with_params_self_evaluates() {
        let clo = Rc::new(Value::Closure(Rc::new(Closure {
            name: "".into(),
            params: vec!["x".into()],
            rest: None,
            body: ident("x"),
            env: Env::new(),
            doc: None,
        })));
        let (v, _) = eval_ok(clo.clone(), &Env::new());
        assert_eq!(v, clo);
    }

    #[test]
    fn let_aliasing_a_function_does_not_invoke_it() {
        // (let f id) where id is a closure: f is bound to the closure, and the
        // form's value is the closure itself — not a call to it.
        let id = Rc::new(Value::Closure(Rc::new(Closure {
            name: "id".into(),
            params: vec!["x".into()],
            rest: None,
            body: ident("x"),
            env: Env::new(),
            doc: None,
        })));
        let env = Env::new().update("id".into(), id.clone());
        let form = list(vec![ident("let"), ident("f"), ident("id")]);
        let (v, env) = eval_ok(form, &env);
        assert!(matches!(&*v, Value::Closure(_)));
        assert_eq!(v, id);
        assert_eq!(lookup(&env, "f"), id);
    }

    /// Build an `Rc<Closure>` for exercising `eval_closure` directly.
    fn closure(params: Vec<Rc<str>>, body: Rc<Value>, env: Env) -> Rc<Closure> {
        Rc::new(Closure {
            name: "".into(),
            params,
            rest: None,
            body,
            env,
            doc: None,
        })
    }

    #[test]
    fn eval_closure_binds_params_then_evaluates_body() {
        let clo = closure(vec!["x".into()], ident("x"), Env::new());
        let (v, _) = eval_closure(&[int(5)], &clo).expect("expected ok");
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn eval_closure_arity_mismatch_errors() {
        let clo = closure(vec!["x".into()], ident("x"), Env::new());
        let err = eval_closure(&[int(1), int(2)], &clo).expect_err("expected err");
        assert!(matches!(
            err,
            RuntimeError::ArityMismatch {
                expected: 1,
                got: 2,
                ..
            }
        ));
    }

    #[test]
    fn eval_closure_resolves_body_against_captured_env() {
        let captured = Env::new().update("z".into(), int(10));
        let clo = closure(vec![], ident("z"), captured);
        let (v, _) = eval_closure(&[], &clo).expect("expected ok");
        assert_eq!(*v, Value::Int(10));
    }

    #[test]
    fn eval_closure_param_shadows_captured_binding() {
        let captured = Env::new().update("x".into(), int(1));
        let clo = closure(vec!["x".into()], ident("x"), captured);
        let (v, _) = eval_closure(&[int(99)], &clo).expect("expected ok");
        assert_eq!(*v, Value::Int(99));
    }

    // ----- fn special form -----

    #[test]
    fn defun_returns_a_closure() {
        // (fn id (x) x)
        let form = list(vec![
            ident(KW_DEFUN),
            ident("id"),
            list(vec![ident("x")]),
            ident("x"),
        ]);
        let (v, _) = eval_ok(form, &Env::new());
        assert!(matches!(
            &*v,
            Value::Closure(c) if c.params.len() == 1 && &*c.params[0] == "x"
        ));
    }

    #[test]
    fn defun_binds_name_and_is_callable() {
        // (fn id (x) x) then (id 5) -> 5
        let def = list(vec![
            ident(KW_DEFUN),
            ident("id"),
            list(vec![ident("x")]),
            ident("x"),
        ]);
        let (_, env) = eval_ok(def, &Env::new());
        assert!(matches!(&*lookup(&env, "id"), Value::Closure(_)));

        let call = list(vec![ident("id"), int(5)]);
        let (v, _) = eval_ok(call, &env);
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn defun_zero_params() {
        // (fn answer () 42) then (answer) -> 42
        let def = list(vec![ident(KW_DEFUN), ident("answer"), unit(), int(42)]);
        let (_, env) = eval_ok(def, &Env::new());
        let (v, _) = eval_ok(list(vec![ident("answer")]), &env);
        assert_eq!(*v, Value::Int(42));
    }

    #[test]
    fn defun_captures_definition_env() {
        // (fn get () z) with z bound at definition -> calling it yields z.
        let env = Env::new().update("z".into(), int(10));
        let def = list(vec![ident(KW_DEFUN), ident("get"), unit(), ident("z")]);
        let (_, env) = eval_ok(def, &env);
        let (v, _) = eval_ok(list(vec![ident("get")]), &env);
        assert_eq!(*v, Value::Int(10));
    }

    #[test]
    fn defun_body_can_reference_itself() {
        // (fn loopy (x) loopy) then (loopy 0) -> the function's own name resolves
        // to its closure inside the body, which is what enables recursion. Without
        // the self-binding this would fail with UnknownIdent("loopy").
        let def = list(vec![
            ident(KW_DEFUN),
            ident("loopy"),
            list(vec![ident("x")]),
            ident("loopy"),
        ]);
        let (_, env) = eval_ok(def, &Env::new());
        let (v, _) = eval_ok(list(vec![ident("loopy"), int(0)]), &env);
        assert!(matches!(&*v, Value::Closure(c) if &*c.name == "loopy"));
    }

    #[test]
    fn defun_non_ident_name_errors() {
        let form = list(vec![
            ident(KW_DEFUN),
            int(5),
            list(vec![ident("x")]),
            ident("x"),
        ]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::TypeMismatch { expected, got, .. }
                if &*expected == "ident" && &*got == "int"
        ));
    }

    #[test]
    fn defun_non_ident_param_errors() {
        let form = list(vec![
            ident(KW_DEFUN),
            ident("f"),
            list(vec![int(1)]),
            ident("x"),
        ]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::TypeMismatch { expected, got, .. }
                if &*expected == "ident" && &*got == "int"
        ));
    }

    #[test]
    fn defun_arity_error() {
        // (fn) is missing params and body — below even the anonymous min.
        let form = list(vec![ident(KW_DEFUN)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::ArityMismatch {
                expected: 3,
                got: 0,
                ..
            }
        ));
    }

    #[test]
    fn defun_anonymous_returns_closure_without_binding() {
        // (fn (x) x): no NAME slot — produces a closure but does not extend env.
        let form = list(vec![ident(KW_DEFUN), list(vec![ident("x")]), ident("x")]);
        let (v, env) = eval_ok(form, &Env::new());
        assert!(matches!(
            &*v,
            Value::Closure(c) if c.params.len() == 1 && &*c.params[0] == "x" && c.name.is_empty()
        ));
        assert!(env.get(&Rc::from("")).is_none());
    }

    #[test]
    fn defun_anonymous_callable_inline() {
        // ((fn (x) x) 7) -> 7
        let lambda = list(vec![ident(KW_DEFUN), list(vec![ident("x")]), ident("x")]);
        let form = list(vec![lambda, int(7)]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(7));
    }

    #[test]
    fn defun_anonymous_with_doc() {
        // (fn (x) (doc "id") x) -> doc attaches to the closure.
        let form = list(vec![
            ident(KW_DEFUN),
            list(vec![ident("x")]),
            list(vec![ident(KW_DOC), string("id")]),
            ident("x"),
        ]);
        let (v, _) = eval_ok(form, &Env::new());
        match &*v {
            Value::Closure(c) => {
                assert!(c.name.is_empty());
                assert_eq!(c.doc.as_deref(), Some("id"));
            }
            other => panic!("expected closure, got {other:?}"),
        }
    }

    #[test]
    fn defun_anonymous_with_variadic_rest() {
        // (fn xs xs) — bare ident params: fully variadic. Calling with (1 2 3)
        // bundles args into a list. Anonymous, so xs is the params name, not
        // the function name.
        let lambda = list(vec![ident(KW_DEFUN), ident("xs"), ident("xs")]);
        let form = list(vec![lambda, int(1), int(2), int(3)]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(v, list(vec![int(1), int(2), int(3)]));
    }

    // ----- if special form -----

    #[test]
    fn if_truthy_cond_evaluates_then_branch() {
        // (if 1 10 20) -> 10
        let form = list(vec![ident(KW_IF), int(1), int(10), int(20)]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(10));
    }

    #[test]
    fn if_falsey_cond_evaluates_else_branch() {
        // (if 0 10 20) -> 20
        let form = list(vec![ident(KW_IF), int(0), int(10), int(20)]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(20));
    }

    #[test]
    fn if_evaluates_its_condition() {
        // (if x 10 20) with x bound to 5 (truthy) -> 10
        let env = Env::new().update("x".into(), int(5));
        let form = list(vec![ident(KW_IF), ident("x"), int(10), int(20)]);
        let (v, _) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(10));
    }

    #[test]
    fn if_does_not_evaluate_untaken_branch() {
        // (if 1 10 undefined): the false branch references an unbound ident, but
        // it must never be evaluated since the condition is truthy.
        let form = list(vec![ident(KW_IF), int(1), int(10), ident("undefined")]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(10));
    }

    #[test]
    fn if_propagates_condition_eval_error() {
        let form = list(vec![ident(KW_IF), ident("undefined"), int(10), int(20)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(err, RuntimeError::UnknownIdent(s) if &*s == "undefined"));
    }

    #[test]
    fn if_arity_error() {
        // (if 1 10) is missing the else branch.
        let form = list(vec![ident(KW_IF), int(1), int(10)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::ArityMismatch {
                expected: 3,
                got: 2,
                ..
            }
        ));
    }

    // ----- do special form -----

    #[test]
    fn empty_do_is_unit() {
        // (do) -> ()
        let form = list(vec![ident(KW_DO)]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Unit);
    }

    #[test]
    fn do_returns_last_form_value() {
        // (do 1 2 3) -> 3
        let form = list(vec![ident(KW_DO), int(1), int(2), int(3)]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(3));
    }

    #[test]
    fn do_threads_env_across_forms() {
        // (do (let x 5) x) -> 5; later form sees the binding from the earlier.
        let form = list(vec![
            ident(KW_DO),
            list(vec![ident(KW_DEFVAR), ident("x"), int(5)]),
            ident("x"),
        ]);
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn do_leaks_scope() {
        let form = list(vec![
            ident(KW_DO),
            list(vec![ident(KW_DEFVAR), ident("x"), int(5)]),
        ]);
        let (_, env) = eval_ok(form, &Env::new());
        assert!(env.get(&Rc::from("x")).is_some_and(|x| *x == int(5)));
    }

    #[test]
    fn let_in_arg_position_does_not_leak_to_caller() {
        // (plus (let x 5) 1) -> 6, but `x` must not leak out of the call.
        let env = Env::new().update("plus".into(), add_builtin());
        let form = list(vec![
            ident("plus"),
            list(vec![ident(KW_DEFVAR), ident("x"), int(5)]),
            int(1),
        ]);
        let (v, env) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(6));
        assert!(env.get(&Rc::from("x")).is_none());
    }

    #[test]
    fn if_branch_bindings_do_not_leak() {
        // (if 1 (let x 5) 0) returns 5 but `x` does not leak to the caller.
        let form = list(vec![
            ident(KW_IF),
            int(1),
            list(vec![ident(KW_DEFVAR), ident("x"), int(5)]),
            int(0),
        ]);
        let (v, env) = eval_ok(form, &Env::new());
        assert_eq!(*v, Value::Int(5));
        assert!(env.get(&Rc::from("x")).is_none());
    }

    #[test]
    fn array_element_bindings_do_not_leak() {
        // [(let x 5) 1] evaluates to [5 1] but `x` does not leak outward.
        let form = array(vec![
            list(vec![ident(KW_DEFVAR), ident("x"), int(5)]),
            int(1),
        ]);
        let (v, env) = eval_ok(form, &Env::new());
        match &*v {
            Value::Array(xs) => {
                assert_eq!(xs.len(), 2);
                assert_eq!(*xs[0], Value::Int(5));
                assert_eq!(*xs[1], Value::Int(1));
            }
            other => panic!("expected array, got {other:?}"),
        }
        assert!(env.get(&Rc::from("x")).is_none());
    }

    #[test]
    fn do_propagates_error_from_a_form() {
        let form = list(vec![ident(KW_DO), int(1), ident("undefined"), int(3)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(err, RuntimeError::UnknownIdent(s) if &*s == "undefined"));
    }

    // ----- quasiquote -----

    fn quasiquote(datum: Rc<Value>) -> Rc<Value> {
        list(vec![ident(KW_QUASIQUOTE), datum])
    }
    fn unquote(datum: Rc<Value>) -> Rc<Value> {
        list(vec![ident(KW_UNQUOTE), datum])
    }
    fn unquote_splicing(datum: Rc<Value>) -> Rc<Value> {
        list(vec![ident(KW_UNQUOTE_SPLICE), datum])
    }

    #[test]
    fn quasiquote_without_unquote_is_literal() {
        // `(1 2 3) -> (1 2 3)
        let form = quasiquote(list(vec![int(1), int(2), int(3)]));
        let (v, _) = eval_ok(form, &Env::new());
        assert_eq!(v, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn quasiquote_atom_returns_it() {
        let (v, _) = eval_ok(quasiquote(int(5)), &Env::new());
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn quasiquote_unquote_evaluates_subform() {
        // `(1 ,x 3) with x = 2 -> (1 2 3)
        let env = Env::new().update("x".into(), int(2));
        let form = quasiquote(list(vec![int(1), unquote(ident("x")), int(3)]));
        let (v, _) = eval_ok(form, &env);
        assert_eq!(v, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn quasiquote_unquote_can_evaluate_a_call() {
        // `(1 ,(plus 1 1)) with plus bound -> (1 2)
        let env = Env::new().update("plus".into(), add_builtin());
        let call = list(vec![ident("plus"), int(1), int(1)]);
        let form = quasiquote(list(vec![int(1), unquote(call)]));
        let (v, _) = eval_ok(form, &env);
        assert_eq!(v, list(vec![int(1), int(2)]));
    }

    #[test]
    fn quasiquote_unquote_splicing_splices_elements() {
        // `(1 ,@xs 4) with xs = (2 3) -> (1 2 3 4)
        let env = Env::new().update("xs".into(), list(vec![int(2), int(3)]));
        let form = quasiquote(list(vec![int(1), unquote_splicing(ident("xs")), int(4)]));
        let (v, _) = eval_ok(form, &env);
        assert_eq!(v, list(vec![int(1), int(2), int(3), int(4)]));
    }

    #[test]
    fn quasiquote_unquote_splicing_empty_list_contributes_nothing() {
        // `(1 ,@xs 2) with xs = () -> (1 2)
        let env = Env::new().update("xs".into(), unit());
        let form = quasiquote(list(vec![int(1), unquote_splicing(ident("xs")), int(2)]));
        let (v, _) = eval_ok(form, &env);
        assert_eq!(v, list(vec![int(1), int(2)]));
    }

    #[test]
    fn quasiquote_recurses_into_nested_lists() {
        // `((1 ,x) 3) with x = 2 -> ((1 2) 3)
        let env = Env::new().update("x".into(), int(2));
        let inner = list(vec![int(1), unquote(ident("x"))]);
        let form = quasiquote(list(vec![inner, int(3)]));
        let (v, _) = eval_ok(form, &env);
        assert_eq!(v, list(vec![list(vec![int(1), int(2)]), int(3)]));
    }

    #[test]
    fn quasiquote_splicing_outside_list_errors() {
        // `,@xs has no surrounding list to splice into.
        let env = Env::new().update("xs".into(), list(vec![int(1)]));
        let form = quasiquote(unquote_splicing(ident("xs")));
        let err = eval_err(form, &env);
        assert!(matches!(
            err,
            RuntimeError::TypeMismatch { name, .. } if &*name == KW_UNQUOTE_SPLICE
        ));
    }

    #[test]
    fn quasiquote_arity_error() {
        let form = list(vec![ident(KW_QUASIQUOTE), ident("a"), ident("b")]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            RuntimeError::ArityMismatch {
                expected: 1,
                got: 2,
                ..
            }
        ));
    }

    // ----- collections -----

    fn array(elems: Vec<Rc<Value>>) -> Rc<Value> {
        Rc::new(Value::Array(elems.iter().cloned().collect()))
    }

    #[test]
    fn array_evaluates_its_elements() {
        let env = Env::new().update("plus".into(), add_builtin());
        // [1, (plus 2 3)] -> [1, 5]
        let call = list(vec![ident("plus"), int(2), int(3)]);
        let (v, _) = eval_ok(array(vec![int(1), call]), &env);
        match &*v {
            Value::Array(xs) => {
                assert_eq!(xs.len(), 2);
                assert_eq!(*xs[0], Value::Int(1));
                assert_eq!(*xs[1], Value::Int(5));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn map_evaluates_its_values() {
        use im::HashMap;
        let env = Env::new().update("plus".into(), add_builtin());
        // {1: (plus 2 3)} -> {1: 5}
        let call = list(vec![ident("plus"), int(2), int(3)]);
        let mut m = HashMap::new();
        m.insert(int(1).clone(), call.clone());
        let (v, _) = eval_ok(Rc::new(Value::Map(m)), &env);
        match &*v {
            Value::Map(m) => {
                assert_eq!(m.len(), 1);
                assert_eq!(
                    m.get(&Value::Int(1)).map(|v| v.deref()),
                    Some(&Value::Int(5))
                );
            }
            other => panic!("expected map, got {other:?}"),
        }
    }

    #[test]
    fn array_in_head_position_is_not_callable() {
        let form = Rc::new(Value::Cons {
            head: array(vec![int(1)]),
            tail: unit(),
        });
        let err = eval_err(form, &Env::new());
        assert!(matches!(err, RuntimeError::NotCallable { .. }));
    }

    #[test]
    fn apply_runs_native_fn_on_evaluated_args() {
        let env = crate::prelude::env();
        let add = env.get(&Rc::from("+")).unwrap().clone();
        let v = apply(
            &add,
            &[Rc::new(Value::Int(2)), Rc::new(Value::Int(3))],
            &env,
        )
        .unwrap();
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn apply_runs_closure() {
        let clo = Rc::new(Value::Closure(Rc::new(Closure {
            name: "id".into(),
            params: vec!["x".into()],
            rest: None,
            body: Rc::new(Value::Ident("x".into())),
            env: Env::new(),
            doc: None,
        })));
        let v = apply(&clo, &[Rc::new(Value::Int(7))], &Env::new()).unwrap();
        assert_eq!(*v, Value::Int(7));
    }

    #[test]
    fn apply_non_callable_errors() {
        let r = apply(&Rc::new(Value::Int(1)), &[], &Env::new());
        assert!(matches!(r, Err(RuntimeError::NotCallable { .. })));
    }
}
