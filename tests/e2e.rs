//! End-to-end tests: source text -> parse -> eval -> value, through the public
//! `parse_and_run` API. Emphasis on nested forms.

use rizz::runtime::Value;
use std::{ops::Deref, rc::Rc};

fn run(src: &str) -> Rc<Value> {
    rizz::parse_and_run(src.as_bytes())
        .map(|(v, _)| v)
        .unwrap_or_else(|e| panic!("eval of `{src}` failed: {e}"))
}

/// Builds a cons-list `Value` of ints, matching how the parser/runtime
/// represent list literals.
fn int_list(xs: &[i64]) -> Value {
    Value::cons_of(xs.iter().copied().map(Value::Int).collect::<Vec<_>>())
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
    assert_eq!(
        *run("(* (+ 1.5 0.5) (- 4.0 1.0))"),
        Value::Float(6.0.into())
    );
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
    assert_eq!(
        *run("(if (< 1 2) (if (> 5 3) 100 200) 300)"),
        Value::Int(100)
    );
    assert_eq!(*run("(if (> 1 2) 999 (if (< 5 3) 200 42))"), Value::Int(42));
}

#[test]
fn and_is_lazy() {
    assert_eq!(
        *run("(let x 0) (and (= (typeof x) 'map) (get x 1))"),
        Value::Int(0)
    );
}

#[test]
fn or_is_lazy() {
    assert_eq!(
        *run("(let x 0) (or (!= (typeof x) 'map) (get x 1))"),
        Value::Int(1)
    );
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
    assert_eq!(
        *run("((fn f (n) (+ ((fn g (n) n) 99) n)) 7)"),
        Value::Int(106)
    );
}

#[test]
fn let_binding_visible_to_later_args() {
    // Application arguments evaluate left-to-right, threading the env, so `x`
    // bound by the first argument is in scope for the second.
    assert_eq!(*run("(+ (let x 5) x)"), Value::Int(10));
}

// ----- variadic functions: dotted-rest params -----

#[test]
fn variadic_rest_collects_trailing_args_as_list() {
    // `rest` is bound to a cons list of the args past the positional ones.
    assert_eq!(
        *run("((fn f (a . rest) rest) 1 2 3 4)"),
        int_list(&[2, 3, 4])
    );
}

#[test]
fn variadic_rest_is_empty_at_minimum_arity() {
    // Exactly the positional count -> rest is the empty list ().
    assert_eq!(*run("((fn f (a . rest) rest) 1)"), Value::Unit);
}

#[test]
fn variadic_bare_ident_params_binds_all_args() {
    // A bare ident in the params position is shorthand for (. args).
    assert_eq!(*run("((fn f args args) 10 20 30)"), int_list(&[10, 20, 30]));
}

#[test]
fn variadic_bare_ident_with_zero_args_is_empty_list() {
    assert_eq!(*run("((fn f args args) )"), Value::Unit);
}

#[test]
fn variadic_too_few_args_errors() {
    // Need at least one positional `a`; calling with none is an arity error.
    let err =
        rizz::parse_and_run("((fn f (a . rest) a))".as_bytes()).expect_err("expected arity error");
    assert!(matches!(
        err,
        rizz::RizzError::RuntimeError(rizz::RuntimeError::ArityMismatch {
            expected: 1,
            got: 0,
            ..
        })
    ));
}

#[test]
fn variadic_rest_works_with_splice() {
    // Splice the bundled rest list back into a call -- a common variadic idiom.
    assert_eq!(
        *run("((fn sum-all args (reduce + 0 args)) 1 2 3 4 5)"),
        Value::Int(15)
    );
}

#[test]
fn variadic_rest_passes_through_reduce() {
    // Common variadic idiom: hand the bundled args to a higher-order fn.
    assert_eq!(
        *run("((fn f xs (reduce + 0 xs)) 1 2 3 4 5 6)"),
        Value::Int(21)
    );
}

// ----- quote / quasiquote with nesting -----

#[test]
fn quote_returns_unevaluated_list() {
    assert_eq!(*run("(quote (1 2 3))"), int_list(&[1, 2, 3]));
    assert_eq!(*run("'(1 2 3)"), int_list(&[1, 2, 3]));
}

#[test]
fn quasiquote_evaluates_nested_unquotes() {
    // `(1 ,(+ 1 1) ,(* 2 3)) -> (1 2 6)
    assert_eq!(
        *run("(quasi (1 (unquote (+ 1 1)) (unquote (* 2 3))))"),
        int_list(&[1, 2, 6])
    );

    assert_eq!(*run("`(1 ,(+ 1 1) ,(* 2 3))"), int_list(&[1, 2, 6]));
}

#[test]
fn quasiquote_unquote_splicing() {
    // `(1 ,@(quote (2 3)) 4) -> (1 2 3 4)
    assert_eq!(
        *run("(quasi (1 (unquote-splice (quote (2 3))) 4))"),
        int_list(&[1, 2, 3, 4])
    );
    assert_eq!(*run("`(1 ,@(quote (2 3)) 4)"), int_list(&[1, 2, 3, 4]));
}

#[test]
fn quasiquote_preserves_nested_list_structure() {
    // `((1 ,(+ 1 1)) 3) -> ((1 2) 3)
    let expected = Value::cons_of(vec![int_list(&[1, 2]), Value::Int(3)]);
    assert_eq!(*run("(quasi ((1 (unquote (+ 1 1))) 3))"), expected);
    assert_eq!(*run("`((1 ,(+ 1 1)) 3)"), expected);
}

// ----- equality over nested values -----

#[test]
fn equality_over_nested_values() {
    assert_eq!(*run("(= (+ 1 1) 2)"), Value::Int(1));
    assert_eq!(*run("(= (quote (1 2)) (quote (1 2)))"), Value::Int(1));
    assert_eq!(*run("(= '(1 2) (quote (1 2)))"), Value::Int(1));
    assert_eq!(*run("(= (quote (1 2)) (quote (1 3)))"), Value::Int(0));
    assert_eq!(*run("(= '(1 2) '(1 3))"), Value::Int(0));
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

// ----- collections -----

#[test]
fn array_literal_evaluates_elements() {
    // Bound via `let` so the array is not in head (call) position; `let`
    // returns the value it bound.
    let v = run("(let xs [1 (+ 1 2) 4])");
    match &*v {
        Value::Array(xs) => {
            assert_eq!(xs.len(), 3);
            assert_eq!(*xs[0], Value::Int(1));
            assert_eq!(*xs[1], Value::Int(3));
            assert_eq!(*xs[2], Value::Int(4));
        }
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn map_literal_evaluates_values() {
    let v = run("(let m {1: (+ 2 3)})");
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
    assert!(rizz::parse_and_run("([1, 2, 3])".as_bytes()).is_err());
}

// ----- errors surface (not panics) through nested forms -----

#[test]
fn unknown_identifier_in_nested_form_is_error() {
    assert!(rizz::parse_and_run("(+ 1 (* 2 nope))".as_bytes()).is_err());
}

#[test]
fn division_by_zero_in_nested_form_is_error() {
    assert!(rizz::parse_and_run("(/ (+ 5 5) (- 3 3))".as_bytes()).is_err());
}

// ----- prelude str/array/map: combined pipelines -----

#[test]
fn reduce_over_mapped_array() {
    // double each then sum: (1+2+3)*2 = 12
    assert_eq!(
        *run("(reduce + 0 (fmap (fn d (x) (* x 2)) [1 2 3]))"),
        Value::Int(12)
    );
}

#[test]
fn join_mapped_to_str() {
    assert_eq!(
        *run("(str-join (fmap to-str (range 1 4)) \",\")"),
        Value::Str("1,2,3".into())
    );
}

#[test]
fn filter_then_len() {
    assert_eq!(
        *run("(len (filter (fn p (x) (> x 2)) (range 0 6)))"),
        Value::Int(3)
    );
}

#[test]
fn map_get_put_roundtrip() {
    // put a key, read it back through the polymorphic get
    assert_eq!(*run("(get (put {1: 2} 3 4) 3)"), Value::Int(4));
}

// ----- implicitly sequenced top-level forms -----

#[test]
fn program_value_is_last_form() {
    // A program is a sequence of forms; the program's value is the last one.
    assert_eq!(*run("(+ 1 2)\n(+ 3 4)"), Value::Int(7));
}

#[test]
fn let_binding_persists_across_top_level_forms() {
    // `let` in form 1 binds `x`; form 2 references it. Without env threading
    // across forms this would fail with UnknownIdent.
    assert_eq!(*run("(let x 10)\n(+ x 5)"), Value::Int(15));
}

#[test]
fn fn_defined_in_one_form_callable_from_the_next() {
    assert_eq!(*run("(fn sq (x) (* x x))\n(sq 6)"), Value::Int(36));
}

#[test]
fn comments_separate_top_level_forms() {
    assert_eq!(
        *run("(let x 1) ;; bind x\n(let y 2) ;; bind y\n(+ x y)"),
        Value::Int(3)
    );
}

// ----- do: explicit sequencing -----

#[test]
fn do_returns_last_form() {
    assert_eq!(*run("(do 1 2 (+ 1 2))"), Value::Int(3));
}

#[test]
fn do_threads_let_to_later_forms() {
    assert_eq!(*run("(do (let x 7) (* x 2))"), Value::Int(14));
}

// ----- ref / deref / set! -----

#[test]
fn ref_deref_roundtrip() {
    assert_eq!(*run("(deref (ref 5))"), Value::Int(5));
    assert_eq!(*run("(deref (ref \"hi\"))"), Value::Str("hi".into()));
}

#[test]
fn set_returns_the_new_value() {
    // Useful for chaining; if you expected unit/old, this is the footgun.
    assert_eq!(*run("(set! (ref 1) 42)"), Value::Int(42));
}

#[test]
fn set_then_deref_sees_the_update() {
    let src = "
        (let r (ref 0))
        (set! r 9)
        (deref r)";
    assert_eq!(*run(src), Value::Int(9));
}

#[test]
fn aliased_bindings_share_the_cell() {
    // `b` is bound to the same ref as `a`; mutation through `a` is visible via `b`.
    let src = "
        (let a (ref 0))
        (let b a)
        (set! a 7)
        (deref b)";
    assert_eq!(*run(src), Value::Int(7));
}

#[test]
fn closure_captures_the_cell_not_a_snapshot() {
    // Canonical opt-in mutability: a counter survives across calls because the
    // closure's captured env holds the *Rc* to the same RefCell.
    let src = "
        (let c (ref 0))
        (fn bump () (set! c (+ (deref c) 1)))
        (bump) (bump) (bump)
        (deref c)";
    assert_eq!(*run(src), Value::Int(3));
}

#[test]
fn set_on_non_ref_errors() {
    assert!(rizz::parse_and_run("(set! 5 1)".as_bytes()).is_err());
    assert!(rizz::parse_and_run("(set! \"x\" 1)".as_bytes()).is_err());
}

#[test]
fn deref_on_non_ref_errors() {
    assert!(rizz::parse_and_run("(deref 5)".as_bytes()).is_err());
    assert!(rizz::parse_and_run("(deref [1 2])".as_bytes()).is_err());
}

#[test]
fn ref_equality_is_identity_not_contents() {
    // Footgun: two refs holding the same value are NOT structurally equal —
    // equality is pointer identity on the cell.
    assert_eq!(*run("(= (ref 5) (ref 5))"), Value::Int(0));
    // A ref equals itself.
    assert_eq!(*run("(let r (ref 5)) (= r r)"), Value::Int(1));
    // Aliased binding still points to the same cell.
    assert_eq!(*run("(let a (ref 5)) (let b a) (= a b)"), Value::Int(1));
}

#[test]
fn ref_truthiness_recurses_into_contents() {
    // Footgun: a ref to a falsy value is itself falsy. Most languages treat any
    // box/handle as truthy; rizz peers through.
    assert_eq!(*run("(if (ref 0) 1 2)"), Value::Int(2));
    assert_eq!(*run("(if (ref \"\") 1 2)"), Value::Int(2));
    assert_eq!(*run("(if (ref ()) 1 2)"), Value::Int(2));
    assert_eq!(*run("(if (ref 1) 1 2)"), Value::Int(1));
}

#[test]
fn refs_auto_deref_through_arithmetic() {
    // Footgun: arithmetic accepts a ref-to-number transparently. There is no
    // explicit `deref` needed, which can hide type confusion.
    assert_eq!(*run("(+ (ref 5) 1)"), Value::Int(6));
    assert_eq!(*run("(+ (ref 5) (ref 7))"), Value::Int(12));
    assert_eq!(*run("(* (ref 3) (ref 4))"), Value::Int(12));
}

#[test]
fn refs_auto_deref_through_comparison() {
    // Same footgun, on comparison operators.
    assert_eq!(*run("(< (ref 1) 2)"), Value::Int(1));
    assert_eq!(*run("(>= (ref 5) (ref 5))"), Value::Int(1));
}

#[test]
fn nested_refs_require_repeated_deref() {
    // (ref x) does NOT auto-collapse if x is already a ref — you get a ref-of-ref.
    assert_eq!(*run("(deref (deref (ref (ref 5))))"), Value::Int(5));
    // One deref leaves you with a ref, which is still truthy/usable.
    let v = run("(deref (ref (ref 5)))");
    assert!(matches!(*v, Value::Ref(_)));
}

#[test]
fn ref_inside_array_is_shared() {
    // Putting a ref into an array stores the same cell; mutating it after
    // construction is visible when you read it back out.
    let src = "
        (let r (ref 1))
        (let xs [r])
        (set! r 99)
        (deref (get xs 0))";
    assert_eq!(*run(src), Value::Int(99));
}

#[test]
fn arithmetic_on_array_ref_element_works_transparently() {
    // Combining the auto-deref footgun with collections.
    let src = "
        (let r (ref 10))
        (let xs [r])
        (set! r 5)
        (+ (get xs 0) 1)";
    assert_eq!(*run(src), Value::Int(6));
}

#[test]
fn set_with_ref_value_creates_alias_not_copy() {
    // Footgun: set! stores the value you hand it; if that value is itself a
    // ref, the cell now holds a ref (not a snapshot of its contents).
    let src = "
        (let inner (ref 1))
        (let outer (ref 0))
        (set! outer inner)
        (set! inner 42)
        (deref (deref outer))";
    assert_eq!(*run(src), Value::Int(42));
}

#[test]
fn closure_keeps_ref_after_outer_binding_shadowed() {
    // The SPEC says closures snapshot their env. The snapshot still contains
    // the same Rc<RefCell>, so a later top-level `(let c ...)` rebinding does
    // not detach the closure from the original cell.
    let src = "
        (let c (ref 0))
        (fn bump () (set! c (+ (deref c) 1)))
        (let c 999)
        (bump)
        (bump)
        (bump)";
    // (bump) returns the post-increment value of the ORIGINAL cell.
    assert_eq!(*run(src), Value::Int(3));
}

#[test]
fn ref_in_head_position_is_callable_if_it_holds_a_fn() {
    // Eval auto-derefs the head when it resolves to a ref-of-callable, so a
    // ref-to-fn can be invoked directly in head position.
    let src = "
        (let f (ref (fn sq (x) (* x x))))
        (f 6)";
    assert_eq!(*run(src), Value::Int(36));

    // Nested ref-of-ref-of-fn also peels.
    let src_nested = "
        (let f (ref (ref (fn sq (x) (* x x)))))
        (f 7)";
    assert_eq!(*run(src_nested), Value::Int(49));

    // A ref holding a non-callable still errors with NotCallable.
    assert!(rizz::parse_and_run("((ref 5))".as_bytes()).is_err());
}

// ----- bang ops on refs: push! / car! / cdr! / put! / del! -----

#[test]
fn push_bang_appends_to_array_held_in_ref() {
    let src = "
        (let r (ref [1 2]))
        (push! r 3)
        (len (deref r))";
    assert_eq!(*run(src), Value::Int(3));
}

#[test]
fn push_bang_returns_the_new_array() {
    assert_eq!(*run("(len (push! (ref [1 2]) 9))"), Value::Int(3));
}

#[test]
fn push_bang_errors_on_non_ref() {
    assert!(rizz::parse_and_run("(push! [1 2] 3)".as_bytes()).is_err());
}

#[test]
fn push_bang_errors_when_ref_holds_non_array() {
    assert!(rizz::parse_and_run("(push! (ref 5) 1)".as_bytes()).is_err());
}

#[test]
fn put_bang_mutates_map_in_ref() {
    let src = "
        (let r (ref {1: 2}))
        (put! r 3 4)
        (get (deref r) 3)";
    assert_eq!(*run(src), Value::Int(4));
}

#[test]
fn put_bang_overwrites_existing_key() {
    let src = "
        (let r (ref {1: 2}))
        (put! r 1 99)
        (get (deref r) 1)";
    assert_eq!(*run(src), Value::Int(99));
}

#[test]
fn del_bang_removes_key_from_ref() {
    let src = "
        (let r (ref {1: 2 3: 4}))
        (del! r 1)
        (len (deref r))";
    assert_eq!(*run(src), Value::Int(1));
}

#[test]
fn del_bang_is_a_noop_when_key_absent() {
    let src = "
        (let r (ref {1: 2}))
        (del! r 99)
        (len (deref r))";
    assert_eq!(*run(src), Value::Int(1));
}

#[test]
fn put_and_del_bang_error_on_wrong_inner_type() {
    assert!(rizz::parse_and_run("(put! (ref [1 2]) 0 9)".as_bytes()).is_err());
    assert!(rizz::parse_and_run("(del! (ref [1 2]) 0)".as_bytes()).is_err());
}

#[test]
fn car_bang_replaces_head_keeps_tail() {
    let src = "
        (let r (ref (cons 1 (cons 2 ()))))
        (car! r 9)
        (car (deref r))";
    assert_eq!(*run(src), Value::Int(9));

    // tail is preserved
    let src_tail = "
        (let r (ref (cons 1 (cons 2 ()))))
        (car! r 9)
        (car (cdr (deref r)))";
    assert_eq!(*run(src_tail), Value::Int(2));
}

#[test]
fn cdr_bang_replaces_tail_keeps_head() {
    let src = "
        (let r (ref (cons 1 (cons 2 ()))))
        (cdr! r (cons 7 ()))
        (car (cdr (deref r)))";
    assert_eq!(*run(src), Value::Int(7));

    // head is preserved
    let src_head = "
        (let r (ref (cons 1 (cons 2 ()))))
        (cdr! r ())
        (car (deref r))";
    assert_eq!(*run(src_head), Value::Int(1));
}

#[test]
fn car_and_cdr_bang_error_on_non_cons() {
    assert!(rizz::parse_and_run("(car! (ref 5) 0)".as_bytes()).is_err());
    assert!(rizz::parse_and_run("(cdr! (ref 5) ())".as_bytes()).is_err());
    // Unit is not a cons cell — there is no slot to replace.
    assert!(rizz::parse_and_run("(car! (ref ()) 0)".as_bytes()).is_err());
}

#[test]
fn bang_ops_are_visible_through_aliases() {
    // Same footgun-as-feature as set!: aliased bindings share the cell, so a
    // mutation through one is seen through the other.
    let src = "
        (let a (ref [1 2]))
        (let b a)
        (push! a 3)
        (len (deref b))";
    assert_eq!(*run(src), Value::Int(3));
}

#[test]
fn do_lets_a_function_body_run_a_sequence() {
    // The original motivation: a fn body can hold a multi-statement sequence
    // by wrapping it in (do ...). The result is the last form's value.
    let src = "
        ((fn run (x)
           (do (let y (* x 2))
               (let z (+ y 1))
               (+ y z)))
         3)";
    assert_eq!(*run(src), Value::Int(13)); // y=6, z=7, y+z=13
}

#[test]
fn zip_e2e() {
    let src = "
        (let a [1 2 3])
        (let b [4 5 6])
        (zip a b)";
    let res = run(src);
    assert_eq!(res.repr(), "([1 4] [2 5] [3 6])");
}

#[test]
fn min_max_clamp_e2e() {
    assert_eq!(*run("(min 5 10)"), Value::Int(5));
    assert_eq!(*run("(max 5 10)"), Value::Int(10));
    assert_eq!(*run("(clamp 7 1 5)"), Value::Int(5));
}

#[test]
fn typeof_returns_correct_type() {
    assert_eq!(*run("(typeof 5)"), Value::Ident("int".into()));
    assert_eq!(*run("(typeof 5.)"), Value::Ident("float".into()));
}

#[test]
fn typeof_returns_quoted() {
    assert_eq!(
        *run(r#"
    (let x 5)
    (let y (typeof x))
    (if (= y 'int)
        1
    (if (= y 'float)
        2))
"#),
        Value::Int(1)
    );
}

// ----- lisp prelude macros: cond / for / loop / while -----

#[test]
fn cond_first_truthy_clause_wins() {
    let src = "(cond ((= 1 2) 10) ((= 2 2) 20) ((= 3 3) 30))";
    assert_eq!(*run(src), Value::Int(20));
}

#[test]
fn cond_else_branch_taken_when_no_match() {
    assert_eq!(*run("(cond ((= 1 2) 10) (else 99))"), Value::Int(99));
}

#[test]
fn cond_no_match_no_else_returns_unit() {
    assert_eq!(*run("(cond ((= 1 2) 10) ((= 3 4) 20))"), Value::Unit);
}

#[test]
fn cond_empty_returns_unit() {
    assert_eq!(*run("(cond)"), Value::Unit);
}

#[test]
fn cond_does_not_evaluate_later_branches() {
    // If `(/ 1 0)` were evaluated we'd get a runtime error. The first clause
    // matches, so the second clause's body is never reached.
    assert_eq!(*run("(cond ((= 1 1) 42) (else (/ 1 0)))"), Value::Int(42));
}

#[test]
fn for_returns_last_body_value() {
    assert_eq!(*run("(for x [10 20 30] x)"), Value::Int(30));
}

#[test]
fn for_over_empty_seq_returns_unit() {
    assert_eq!(*run("(for x [] x)"), Value::Unit);
}

#[test]
fn for_iterates_via_ref_accumulator() {
    let src = "
        (let! sum 0)
        (for x [1 2 3 4] (set! sum (+ sum x)))
        (deref sum)";
    assert_eq!(*run(src), Value::Int(10));
}

#[test]
fn for_iterates_over_list() {
    let src = "
        (let! sum 0)
        (for x '(5 10 15) (set! sum (+ sum x)))
        (deref sum)";
    assert_eq!(*run(src), Value::Int(30));
}

#[test]
fn for_binds_loop_var_in_body() {
    let src = "
        (let! last 0)
        (for x [7 8 9] (set! last x))
        (deref last)";
    assert_eq!(*run(src), Value::Int(9));
}

#[test]
fn loop_returns_last_body_value() {
    assert_eq!(*run("(loop 5 (+ 1 1))"), Value::Int(2));
}

#[test]
fn loop_zero_iterations_returns_unit() {
    assert_eq!(*run("(loop 0 99)"), Value::Unit);
}

#[test]
fn loop_runs_n_times() {
    let src = "
        (let! c 0)
        (loop 7 (set! c (+ c 1)))
        (deref c)";
    assert_eq!(*run(src), Value::Int(7));
}

#[test]
fn while_runs_until_cond_falsy() {
    let src = "
        (let! i 0)
        (let! sum 0)
        (while (< i 5)
          (set! sum (+ sum i))
          (set! i (+ i 1)))
        (deref sum)";
    assert_eq!(*run(src), Value::Int(10));
}

#[test]
fn while_returns_last_body_value() {
    let src = "
        (let! i 0)
        (while (< i 3) (set! i (+ i 1)))";
    assert_eq!(*run(src), Value::Int(3));
}

#[test]
fn while_returns_unit_when_cond_initially_false() {
    assert_eq!(*run("(while (= 1 2) 99)"), Value::Unit);
}

// ----- doc / show -----

#[test]
fn show_on_undocumented_fn_is_unit() {
    let src = r#"
        (fn inc (n) (+ n 1))
        (show inc)"#;
    assert_eq!(*run(src), Value::Unit);
}

#[test]
fn fn_doc_is_retrievable_via_show() {
    let src = r#"
        (fn inc (n)
          (doc "increments a number by 1")
          (+ n 1))
        (show inc)"#;
    assert_eq!(*run(src), Value::Str("increments a number by 1".into()));
}

#[test]
fn fn_doc_strings_are_newline_joined() {
    let src = r#"
        (fn inc (n)
          (doc "line one" "line two" "line three")
          (+ n 1))
        (show inc)"#;
    assert_eq!(
        *run(src),
        Value::Str("line one\nline two\nline three".into())
    );
}

#[test]
fn documented_fn_still_callable() {
    let src = r#"
        (fn inc (n) (doc "+1") (+ n 1))
        (inc 4)"#;
    assert_eq!(*run(src), Value::Int(5));
}

#[test]
fn defmacro_doc_is_retrievable() {
    let src = r#"
        (defmacro when (c . body)
          (doc "if-without-else")
          `(if ,c (do ,@body) ()))
        (show when)"#;
    assert_eq!(*run(src), Value::Str("if-without-else".into()));
}

#[test]
fn documented_macro_still_expands() {
    let src = r#"
        (defmacro when (c . body)
          (doc "if-without-else")
          `(if ,c (do ,@body) ()))
        (when 1 42)"#;
    assert_eq!(*run(src), Value::Int(42));
}

#[test]
fn let_doc_on_callable_is_attached() {
    // `let` doc applies when the bound value is a callable.
    let src = r#"
        (let inc (fn _inner (n) (+ n 1)))
        (let documented (doc "wrapped inc") inc)
        (show documented)"#;
    assert_eq!(*run(src), Value::Str("wrapped inc".into()));
}

#[test]
fn let_doc_on_non_callable_is_dropped() {
    // Non-callable values have no doc slot; the doc is silently discarded.
    let src = r#"
        (let pi (doc "approx of pi") 3.14)
        (show pi)"#;
    assert_eq!(*run(src), Value::Unit);
}

#[test]
fn let_ref_doc_on_callable_is_attached() {
    // For let!, the doc attaches to the underlying value before it is wrapped
    // in a ref; show peels the ref.
    let src = r#"
        (let! inc (doc "bumps n") (fn _inner (n) (+ n 1)))
        (show inc)"#;
    assert_eq!(*run(src), Value::Str("bumps n".into()));
}

#[test]
fn show_on_documented_builtin_returns_doc() {
    let v = run("(show +)");
    let s = match &*v {
        Value::Str(s) => s.clone(),
        other => panic!("expected str, got {other}"),
    };
    assert!(s.contains("(+ a b)"), "doc string was {s:?}");
}

#[test]
fn doc_form_with_non_string_arg_errors() {
    let src = r#"(fn inc (n) (doc 42) (+ n 1))"#;
    assert!(rizz::parse_and_run(src.as_bytes()).is_err());
}

#[test]
fn doc_form_with_no_strings_errors() {
    let src = r#"(fn inc (n) (doc) (+ n 1))"#;
    assert!(rizz::parse_and_run(src.as_bytes()).is_err());
}

#[test]
fn extra_arg_in_let_still_arity_error() {
    // Adding the doc slot must not regress the existing arity diagnostic for
    // accidental extra args.
    let src = r#"(let x 1 2)"#;
    assert!(rizz::parse_and_run(src.as_bytes()).is_err());
}
