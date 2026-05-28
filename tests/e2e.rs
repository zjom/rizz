//! End-to-end tests: source text -> parse -> eval -> value, through the public
//! `parse_and_run` API. Emphasis on nested forms.

use lirsp::evaluator::Value;
use std::rc::Rc;

fn run(src: &str) -> Rc<Value> {
    lirsp::parse_and_run(src.as_bytes())
        .map(|(v, _)| v)
        .unwrap_or_else(|e| panic!("eval of `{src}` failed: {e}"))
}

/// Builds a cons-list `Value` of ints, matching how the parser/evaluator
/// represent list literals.
fn int_list(xs: &[i64]) -> Value {
    Value::from(xs.iter().copied().map(Value::Int).collect::<Vec<_>>())
}

// ----- nested arithmetic -----

#[test]
fn nested_arithmetic() {
    assert_eq!(*run("(+ (* 2 3) (- 10 4))"), Value::Int(12));
    assert_eq!(*run("(* (+ 1 2) (+ 3 4))"), Value::Int(21));
    assert_eq!(*run("(- (+ 100 (* 5 5)) (/ 50 2))"), Value::Int(100));
}

#[test]
fn deeply_nested_arithmetic() {
    assert_eq!(*run("(+ 1 (+ 2 (+ 3 (+ 4 (+ 5 6)))))"), Value::Int(21));
}

#[test]
fn nested_float_arithmetic() {
    assert_eq!(*run("(* (+ 1.5 0.5) (- 4.0 1.0))"), Value::Float(6.0));
}

// ----- nested comparisons -----

#[test]
fn comparisons_over_computed_operands() {
    assert_eq!(*run("(< (+ 1 1) (* 2 2))"), Value::Int(1)); // 2 < 4
    assert_eq!(*run("(>= (* 3 3) (+ 4 5))"), Value::Int(1)); // 9 >= 9
    assert_eq!(*run("(<= (+ 5 5) (- 9 1))"), Value::Int(0)); // 10 <= 8
}

// ----- nested if -----

#[test]
fn nested_if_selects_correct_branch() {
    assert_eq!(*run("(if (< 1 2) (+ 10 20) (- 0 1))"), Value::Int(30));
    assert_eq!(*run("(if (> 1 2) (+ 10 20) (* 6 7))"), Value::Int(42));
    assert_eq!(*run("(if (< 1 2) (if (> 5 3) 100 200) 300)"), Value::Int(100));
    assert_eq!(*run("(if (> 1 2) 999 (if (< 5 3) 200 42))"), Value::Int(42));
}

// ----- functions, application, recursion -----

#[test]
fn inline_function_application() {
    assert_eq!(*run("((fn sq (x) (* x x)) 5)"), Value::Int(25));
}

#[test]
fn recursive_factorial() {
    assert_eq!(
        *run("((fn fact (n) (if (< n 1) 1 (* n (fact (- n 1))))) 5)"),
        Value::Int(120)
    );
}

#[test]
fn recursive_fibonacci() {
    assert_eq!(
        *run("((fn fib (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) 10)"),
        Value::Int(55)
    );
}

#[test]
fn callee_bindings_do_not_leak_to_caller() {
    // The inner call binds its own `n` = 99; evaluating it as the first argument
    // must not change the outer `n` = 7 seen by the second argument.
    // Expected (+ 99 7) = 106; a leaking env would yield (+ 99 99) = 198.
    assert_eq!(*run("((fn f (n) (+ ((fn g (n) n) 99) n)) 7)"), Value::Int(106));
}

#[test]
fn let_binding_visible_to_later_args() {
    // Application arguments evaluate left-to-right, threading the env, so `x`
    // bound by the first argument is in scope for the second.
    assert_eq!(*run("(+ (let x 5) x)"), Value::Int(10));
}

// ----- quote / quasiquote with nesting -----

#[test]
fn quote_returns_unevaluated_list() {
    assert_eq!(*run("(quote (1 2 3))"), int_list(&[1, 2, 3]));
}

#[test]
fn quasiquote_evaluates_nested_unquotes() {
    // `(1 ,(+ 1 1) ,(* 2 3)) -> (1 2 6)
    assert_eq!(
        *run("(quasi (1 (unquote (+ 1 1)) (unquote (* 2 3))))"),
        int_list(&[1, 2, 6])
    );
}

#[test]
fn quasiquote_unquote_splicing() {
    // `(1 ,@(quote (2 3)) 4) -> (1 2 3 4)
    assert_eq!(
        *run("(quasi (1 (unquote-splice (quote (2 3))) 4))"),
        int_list(&[1, 2, 3, 4])
    );
}

#[test]
fn quasiquote_preserves_nested_list_structure() {
    // `((1 ,(+ 1 1)) 3) -> ((1 2) 3)
    let expected = Value::from(vec![int_list(&[1, 2]), Value::Int(3)]);
    assert_eq!(*run("(quasi ((1 (unquote (+ 1 1))) 3))"), expected);
}

// ----- equality over nested values -----

#[test]
fn equality_over_nested_values() {
    assert_eq!(*run("(= (+ 1 1) 2)"), Value::Int(1));
    assert_eq!(*run("(= (quote (1 2)) (quote (1 2)))"), Value::Int(1));
    assert_eq!(*run("(= (quote (1 2)) (quote (1 3)))"), Value::Int(0));
}

// ----- a program combining several forms -----

#[test]
fn combined_nested_program() {
    // if the computed condition holds, square a computed argument, else 0.
    assert_eq!(
        *run("(if (= (+ 1 1) 2) ((fn sq (x) (* x x)) (+ 2 1)) 0)"),
        Value::Int(9)
    );
}

// ----- errors surface (not panics) through nested forms -----

#[test]
fn unknown_identifier_in_nested_form_is_error() {
    assert!(lirsp::parse_and_run("(+ 1 (* 2 nope))".as_bytes()).is_err());
}

#[test]
fn division_by_zero_in_nested_form_is_error() {
    assert!(lirsp::parse_and_run("(/ (+ 5 5) (- 3 3))".as_bytes()).is_err());
}
