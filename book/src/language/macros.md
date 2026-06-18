# Macros and Metaprogramming

Macros are functions that run at the *call site* on **unevaluated** argument
forms, and whose result is then evaluated in the caller's environment. Because
rizz code is just data (cons lists and idents), a macro is a program that
transforms one chunk of code into another. Every control-flow form you met in
[Control Flow](control-flow.md) is a macro.

## `defmacro`

```clojure
(defmacro NAME PARAMS BODY)
```

The shape mirrors [`fn`](functions.md) — including fixed, dotted-rest, and
bare-ident parameter lists — with one essential difference: the arguments arrive
**unevaluated**. The body computes a new form (typically with
[quasiquote](special-forms.md)), and *that* form is evaluated in the caller's
scope.

Here is `unless`, essentially as the prelude defines it:

```clojure
(defmacro unless (cond . body)
  `(if ,cond () (do ,@body)))

(unless (> 1 2) 'ok)   ;; => ok
```

When you write `(unless (> 1 2) 'ok)`, the macro receives `cond = (> 1 2)` and
`body = ('ok)` as *data*. It builds `(if (> 1 2) () (do 'ok))`, which is then
evaluated. Note the body forms were never evaluated by the macro itself — only
the expansion is.

## Quasiquote is the assembly tool

Macros are mostly quasiquote with the macro's parameters spliced in:

- `` ` `` starts a template that is literal by default.
- `,x` drops in the *value* of `x` (here, the argument form the macro received).
- `,@xs` splices a *sequence* of forms into the surrounding list.

```clojure
(defmacro swap-args (f a b)
  `(,f ,b ,a))

(swap-args - 3 10)     ;; expands to (- 10 3) => 7
```

If you are ever unsure what a macro expands to, remember the pieces are ordinary
data — you can build and inspect expansions with `quote`, `car`, `cdr`, and
friends.

## Macros discard their bindings

A macro's expansion is evaluated, but **any bindings it introduces while
expanding are discarded** — they do not leak into the caller. A macro that
expands to `(let x 1)` does *not* bind `x` in the surrounding scope:

```clojure
(defmacro def-x () `(let x 1))
(def-x)
x                  ;; UnknownIdent — x did not escape the macro
```

If a macro genuinely needs to carry state into the caller, it should expand to
operations on a [ref](refs.md) the caller can see, not to a `let`.

This is also why the looping macros (`for`, `loop`, `while`) cannot accumulate
into a plain `let` — they thread results through `reduce` or a captured ref
instead.

## Hygiene by convention: the `__` prefix

rizz macros are **not hygienic** — there is no automatic renaming of
macro-introduced names. If your expansion binds a temporary and the user happens
to use the same name, they can collide. The prelude's convention is to give
macro-internal names a `__` prefix, making accidental capture unlikely.

You can see this throughout `_.rz`. Here is `or`, which evaluates its first
argument exactly once by binding it to `__a`:

```clojure
(defmacro or (a b)
  `((fn __or (__a) (if __a __a ,b)) ,a))
```

The expansion wraps `a` in a one-shot function so it is evaluated a single time,
names it `__a`, and only mentions `b` in the else position so it is evaluated
lazily. The `__a` name is unlikely to clash with anything the caller wrote.

When you write your own macros, follow the same convention: prefix any
temporary the macro introduces with `__`.

## Worked example: how `cond` is built

`cond` is a recursive macro that expands into nested `if`s. Stripped to its
logic:

```clojure
(defmacro cond clauses
  (if (= clauses ())
      ()
      (if (= (car (car clauses)) 'else)
          (car (cdr (car clauses)))                  ;; (else EXPR) → EXPR
          `(if ,(car (car clauses))                  ;; TEST
               ,(car (cdr (car clauses)))            ;; BODY
               (cond ,@(cdr clauses))))))            ;; recurse on the rest
```

So `(cond (A x) (B y) (else z))` expands to
`(if A x (if B y z))`. The macro pulls apart each clause with `car`/`cdr`,
emits an `if`, and recursively expands the remaining clauses by splicing
`,@(cdr clauses)` back into a `cond`. The recursion happens at *expansion* time;
the final code is a plain `if` ladder.

`match`, `try-with`, and `while` are built the same way — small recursive macros
that emit `if`/`fn`/`do`. Reading `src/prelude/_.rz` is the best way to learn
idiomatic macro style.

## `eval` — the other half of metaprogramming

Where macros transform code *before* it runs, [`eval`](special-forms.md) runs
data *as* code at runtime:

```clojure
(let program '(+ 1 2))
(eval program)     ;; => 3
```

Combined with `quote`/quasiquote and the list operations, `eval` lets you build
and run programs from data — useful for interpreters and data-driven dispatch,
though a macro or an ordinary higher-order function is usually clearer for code
you write by hand.

## When to use a macro

Reach for a macro only when you need to control *evaluation* — to leave
something unevaluated, to introduce binding syntax, or to build new control
flow. If a plain function would do (because all its arguments should be
evaluated anyway), write the function: functions compose, can be passed around,
and are easier to reason about. Macros buy you syntactic power at the cost of
being second-class (they cannot be `apply`'d or stored as ordinary callables).

---

*See also:* [Special Forms](special-forms.md) · [Control Flow](control-flow.md) ·
[Refs and Mutability](refs.md) · [Errors and Exceptions](errors.md) ·
*SPEC.md* §5.4, §5.9
