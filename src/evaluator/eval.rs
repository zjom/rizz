use crate::evaluator::{Env, EvaluatorError, Value};
use std::rc::Rc;

const KW_LET: &str = "let";
const KW_QUOTE: &str = "quote";
const KW_QUASIQUOTE: &str = "quasi";
const KW_UNQUOTE: &str = "unquote";
const KW_UNQUOTE_SPLICE: &str = "unquote-splice";

pub fn eval(form: Rc<Value>, ctx: &Env) -> Result<(Rc<Value>, Env), EvaluatorError> {
    match &*form {
        Value::Int(_) | Value::Unit | Value::Str(_) | Value::Float(_) | Value::BuiltinFn(_) => {
            Ok((form.clone(), ctx.clone()))
        }
        Value::Ident(ident) => {
            let form = ctx
                .get(ident)
                .ok_or(EvaluatorError::UnknownIdent(ident.clone()))?;

            Ok((form.clone(), ctx.clone()))
        }
        Value::Cons { head, tail } => {
            if let Value::Ident(ident) = &**head {
                match ident.as_ref() {
                    KW_LET => {
                        let (v, env) = eval_assign(tail, ctx)?;
                        return eval(v, &env);
                    }
                    KW_QUOTE => {
                        let (v, env) = eval_quote(tail, ctx)?;
                        return Ok((v, env.clone()));
                    }
                    KW_QUASIQUOTE => return eval_quasiquote(tail, ctx),
                    _ => {}
                }
            }
            let mut args = Vec::new();
            let mut env = ctx.clone();
            for arg in Value::iter(tail) {
                let (val, env2) = eval(arg.clone(), &env)?;
                args.push(val);
                env = env2;
            }
            let (callable, ctx) = eval(head.clone(), ctx)?;
            match &*callable {
                Value::BuiltinFn(f) => {
                    let (v, env) = f(&args, &ctx)?;
                    eval(v, &env)
                }
                Value::Closure { params, body, env } => eval_closure(&args, params, body, env),
                Value::Int(_)
                | Value::Unit
                | Value::Str(_)
                | Value::Float(_)
                | Value::Cons { .. }
                | Value::Ident(_) => Err(EvaluatorError::NotCallable { value: callable }),
            }
        }
        Value::Closure { params, body, env } => eval_closure(&[], params, body, env),
    }
}

fn eval_closure(
    args: &[Rc<Value>],
    params: &[Rc<str>],
    body: &Rc<Value>,
    env: &Env,
) -> Result<(Rc<Value>, Env), EvaluatorError> {
    if params.len() != args.len() {
        return Err(EvaluatorError::ArityMismatch {
            name: "<closure>".into(),
            expected: params.len(),
            got: args.len(),
        });
    }

    let mut call_env = env.clone();
    for (ident, arg) in params.iter().zip(args) {
        call_env = call_env.update(ident.clone(), arg.clone());
    }
    eval(body.clone(), &call_env)
}

fn eval_assign(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), EvaluatorError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 2 {
        return Err(EvaluatorError::ArityMismatch {
            name: KW_LET.into(),
            expected: 2,
            got: items.len(),
        });
    }
    let Value::Ident(name) = &*items[0] else {
        return Err(EvaluatorError::TypeMismatch {
            name: KW_LET.into(),
            expected: "ident".into(),
            got: Value::type_name(&items[0]).into(),
        });
    };

    let (val, env) = eval(items[1].clone(), env)?;
    let env = env.update(name.clone(), val.clone());
    Ok((val, env))
}

fn eval_quote(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), EvaluatorError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 1 {
        return Err(EvaluatorError::ArityMismatch {
            name: KW_QUOTE.into(),
            expected: 1,
            got: items.len(),
        });
    }

    Ok((items[0].clone(), env.clone()))
}

fn eval_quasiquote(tail: &Rc<Value>, env: &Env) -> Result<(Rc<Value>, Env), EvaluatorError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 1 {
        return Err(EvaluatorError::ArityMismatch {
            name: KW_QUASIQUOTE.into(),
            expected: 1,
            got: items.len(),
        });
    }

    Ok((quasi(&items[0], env)?, env.clone()))
}

/// Rebuild `datum` as a literal structure, evaluating any `(unquote X)` subform
/// and splicing the elements of any `(unquote-splicing X)` element.
fn quasi(datum: &Rc<Value>, env: &Env) -> Result<Rc<Value>, EvaluatorError> {
    if let Some(tail) = tagged(datum, KW_UNQUOTE) {
        return Ok(eval(unquote_operand(KW_UNQUOTE, tail)?, env)?.0);
    }
    if tagged(datum, KW_UNQUOTE_SPLICE).is_some() {
        return Err(EvaluatorError::TypeMismatch {
            name: KW_UNQUOTE_SPLICE.into(),
            expected: "list context".into(),
            got: KW_QUASIQUOTE.into(),
        });
    }
    let Value::Cons { .. } = &**datum else {
        return Ok(datum.clone());
    };

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

fn unquote_operand(name: &'static str, tail: &Rc<Value>) -> Result<Rc<Value>, EvaluatorError> {
    let items: Vec<_> = Value::iter(tail).collect();
    if items.len() != 1 {
        return Err(EvaluatorError::ArityMismatch {
            name: name.into(),
            expected: 1,
            got: items.len(),
        });
    }
    Ok(items[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- helpers -----

    fn int(n: i64) -> Rc<Value> {
        Rc::new(Value::Int(n))
    }
    fn float(f: f64) -> Rc<Value> {
        Rc::new(Value::Float(f))
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

    /// A two-arg integer-add builtin that enforces its arity, used to drive the
    /// application arms of `eval`.
    fn add_builtin() -> Rc<Value> {
        Rc::new(Value::BuiltinFn(Rc::new(
            |args: &[Rc<Value>], env: &Env| -> Result<(Rc<Value>, Env), EvaluatorError> {
                if args.len() != 2 {
                    return Err(EvaluatorError::ArityMismatch {
                        name: "plus".into(),
                        expected: 2,
                        got: args.len(),
                    });
                }
                let a = args[0].as_int().expect("int");
                let b = args[1].as_int().expect("int");
                Ok((int(a + b), env.clone()))
            },
        )))
    }

    fn eval_ok(form: Rc<Value>, env: &Env) -> (Rc<Value>, Env) {
        eval(form, env).expect("expected successful eval")
    }
    fn eval_err(form: Rc<Value>, env: &Env) -> EvaluatorError {
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
        assert_eq!(*v, Value::Float(3.5));
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
        assert!(matches!(&*v, Value::BuiltinFn(_)));
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
        assert!(matches!(err, EvaluatorError::UnknownIdent(s) if &*s == "nope"));
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
            EvaluatorError::ArityMismatch {
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
            EvaluatorError::ArityMismatch {
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
            EvaluatorError::TypeMismatch { expected, got, .. }
                if &*expected == "ident" && &*got == "int"
        ));
    }

    #[test]
    fn let_propagates_value_eval_error() {
        // (let x undefined) -> evaluating the value expr fails.
        let form = list(vec![ident("let"), ident("x"), ident("undefined")]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(err, EvaluatorError::UnknownIdent(s) if &*s == "undefined"));
    }

    // ----- cons head dispatch -----

    #[test]
    fn unbound_ident_in_head_position_errors() {
        // A non-special-form head ident falls through to application, where it
        // is looked up like any other ident and fails if unbound.
        let form = list(vec![ident("frobnicate"), int(1)]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(err, EvaluatorError::UnknownIdent(s) if &*s == "frobnicate"));
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
        let id = Rc::new(Value::Closure {
            params: vec!["x".into()],
            body: ident("x"),
            env: Env::new(),
        });
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
        let head = list(vec![ident(KW_LET), ident("f"), ident("plus")]);
        let form = list(vec![head, int(1), int(2)]);
        let (v, _) = eval_ok(form, &env);
        assert_eq!(*v, Value::Int(3));
    }

    #[test]
    fn application_evaluates_its_arguments() {
        // (plus x 2) with x bound to 40 -> 42; arguments are evaluated first.
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
            EvaluatorError::ArityMismatch {
                expected: 2,
                got: 1,
                ..
            }
        ));
    }

    // ----- closures -----

    #[test]
    fn zero_arg_closure_form_evaluates_its_body() {
        let clo = Rc::new(Value::Closure {
            params: vec![],
            body: int(7),
            env: Env::new(),
        });
        let (v, _) = eval_ok(clo, &Env::new());
        assert_eq!(*v, Value::Int(7));
    }

    #[test]
    fn closure_form_with_params_errors_on_zero_args() {
        let clo = Rc::new(Value::Closure {
            params: vec!["x".into()],
            body: ident("x"),
            env: Env::new(),
        });
        let err = eval_err(clo, &Env::new());
        assert!(matches!(
            err,
            EvaluatorError::ArityMismatch {
                expected: 1,
                got: 0,
                ..
            }
        ));
    }

    #[test]
    fn eval_closure_binds_params_then_evaluates_body() {
        let params: Vec<Rc<str>> = vec!["x".into()];
        let (v, _) =
            eval_closure(&[int(5)], &params, &ident("x"), &Env::new()).expect("expected ok");
        assert_eq!(*v, Value::Int(5));
    }

    #[test]
    fn eval_closure_arity_mismatch_errors() {
        let params: Vec<Rc<str>> = vec!["x".into()];
        let err = eval_closure(&[int(1), int(2)], &params, &ident("x"), &Env::new())
            .expect_err("expected err");
        assert!(matches!(
            err,
            EvaluatorError::ArityMismatch {
                expected: 1,
                got: 2,
                ..
            }
        ));
    }

    #[test]
    fn eval_closure_resolves_body_against_captured_env() {
        let captured = Env::new().update("z".into(), int(10));
        let (v, _) = eval_closure(&[], &[], &ident("z"), &captured).expect("expected ok");
        assert_eq!(*v, Value::Int(10));
    }

    #[test]
    fn eval_closure_param_shadows_captured_binding() {
        let captured = Env::new().update("x".into(), int(1));
        let params: Vec<Rc<str>> = vec!["x".into()];
        let (v, _) =
            eval_closure(&[int(99)], &params, &ident("x"), &captured).expect("expected ok");
        assert_eq!(*v, Value::Int(99));
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
            EvaluatorError::TypeMismatch { name, .. } if &*name == KW_UNQUOTE_SPLICE
        ));
    }

    #[test]
    fn quasiquote_arity_error() {
        let form = list(vec![ident(KW_QUASIQUOTE), ident("a"), ident("b")]);
        let err = eval_err(form, &Env::new());
        assert!(matches!(
            err,
            EvaluatorError::ArityMismatch {
                expected: 1,
                got: 2,
                ..
            }
        ));
    }
}
