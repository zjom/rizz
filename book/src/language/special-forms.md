# Special Forms

A **special form** is a list whose head is a reserved keyword, dispatched by the
evaluator *before* normal function application. Special forms do not follow the
"evaluate all arguments, then call" rule — each defines its own evaluation
behavior. That is precisely why they cannot be ordinary functions.

You have already met the binding special forms (`let`, `let!`, `fn`) in
[Bindings and Functions](functions.md), and `try`/`exception` live in
[Errors and Exceptions](errors.md). This chapter covers the rest:
`if`, `do`, `quote`, quasiquotation, and `eval`.

## `if` — conditional

```clojure
(if COND THEN ELSE)
```

`if` evaluates `COND`, and then evaluates **only** the taken branch. The untaken
branch is never touched — which is what makes `if` a special form rather than a
function:

```clojure
(if (< 1 2) 'yes 'no)     ;; => yes
(if 0 (/ 1 0) 'safe)      ;; => safe  — the divide is never evaluated
```

Truthiness follows the rules in [Values and Types](values.md): `()`, `0`, `0.0`,
`""`, `[]`, and `{}` are false; everything else is true. `if` requires exactly
three arguments. For a two-armed conditional with no else, see
[`unless`](control-flow.md) or use `()` as the else branch.

## `do` — sequencing

```clojure
(do FORM*)
```

`do` evaluates each form in order, threading the environment between them, and
returns the value of the last (or `()` if empty). Its primary use is giving a
function a multi-step body:

```clojure
((fn run (x)
   (do (let y (* x 2))
       (let z (+ y 1))
       (+ y z)))
 3)                  ;; => 13
```

The crucial property of `do` is that **it is not a scope boundary**. A later
form sees the `let`/`fn` bindings of earlier forms, *and those bindings leak out
to the surrounding form too*:

```clojure
(do (let a 1) (let b 2))
(+ a b)              ;; => 3   — a and b escaped the do
```

Think of `do` as splicing its forms into the enclosing position. This is exactly
how the top level threads bindings between forms — wrapping a sequence in `do`
reproduces that same visibility inside an expression.

## `quote` — literal data

```clojure
(quote X)     ;; or 'X
```

`quote` returns its argument **unevaluated**, as data. Identifiers come back as
`ident` values; lists come back as cons chains:

```clojure
'foo            ;; => the ident foo
'(+ 1 2)        ;; => the 3-element list (+ 1 2), NOT 3
(car '(+ 1 2))  ;; => the ident +
```

Quoting is how you get your hands on code-as-data — the foundation of
[macros](macros.md).

## Quasiquotation — `quasi`, `unquote`, `unquote-splice`

Quasiquote is "quote with holes." `` `X `` is mostly literal, but:

- `,X` (unquote) — evaluate `X` and drop its value in.
- `,@X` (unquote-splice) — evaluate `X` to a **cons list** and **splice** its
  elements into the surrounding list.

```clojure
(let n 2)
`(a ,n c)                ;; => (a 2 c)
`(1 ,(+ 1 1) ,@'(3 4 5)) ;; => (1 2 3 4 5)
```

Splicing flattens a cons list specifically — an array splices as a *single*
element, not flattened, so reach for a list (or `'(...)`) when you want the
elements spread. Splicing only makes sense as an *element of a list*;
`` `,@xs `` with no enclosing list is a type error. Quasiquote recurses into
nested lists, and this
implementation does not track nested-quasiquote depth — an unquote always
applies to the nearest enclosing quasiquote.

Quasiquote is the workhorse of macro definitions, where you assemble an
expansion out of literal structure plus spliced-in argument forms. See
[Macros and Metaprogramming](macros.md).

## `eval` — run data as code

```clojure
(eval FORM)
```

`eval` evaluates `FORM` once to get a datum, then evaluates *that datum* as code
in the current environment:

```clojure
(let three '(+ 1 2))
(eval three)         ;; => 3
```

Like `do`, `eval` is not a scope boundary — bindings the evaluated code
introduces extend the caller's scope. `eval` is occasionally the right tool for
interpreting data-driven programs, but in everyday code a function or macro is
almost always clearer.

---

*See also:* [The Evaluation Model](evaluation.md) ·
[Control Flow](control-flow.md) · [Macros and Metaprogramming](macros.md) ·
[Errors and Exceptions](errors.md) · *SPEC.md* §5
