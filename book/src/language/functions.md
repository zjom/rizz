# Bindings and Functions

This chapter covers the binding forms — `let`, `let!`, and `fn` — and everything
about functions: parameters, variadics, anonymous functions, and recursion.

## `let` — bind a value

`let` evaluates an expression and binds the result to a name in the surrounding
environment. It returns the bound value.

```clojure
(let x 10)
(let y (* x 2))
y                  ;; => 20
```

A name bound by `let` is visible to every later form in the same scope (see
[Evaluation](evaluation.md)). `let` can also carry a documentation string; that
optional slot is covered in [Documentation](documentation.md).

## `let!` — bind a fresh ref

`let!` is sugar for binding a [ref](refs.md): `(let! c 0)` is exactly
`(let c (ref 0))`. Reach for it whenever you want a name you will mutate with
`set!`, `push!`, and the like:

```clojure
(let! c 0)
(set! c (+ (deref c) 1))
(deref c)          ;; => 1
```

We will not belabor refs here — [Refs and Mutability](refs.md) is their chapter.

## `fn` — define a function

The common shape is a name, a parameter list, and a body:

```clojure
(fn square (x) (* x x))
(square 5)         ;; => 25
```

`fn` returns the closure, *and* (when named) binds it under that name in the
surrounding environment. The body is a **single form** — use [`do`](special-forms.md)
to sequence several steps:

```clojure
(fn stats (n)
  (do
    (let sq (* n n))
    (let cube (* sq n))
    [sq cube]))
(stats 3)          ;; => [9 27]
```

### Parameters

The parameter list is a list of identifiers; use `()` for no parameters:

```clojure
(fn pi () 3.14159)
(fn add (a b) (+ a b))
(add 2 3)          ;; => 5
```

For a fixed-arity function the argument count must match exactly — too few or
too many is an `ArityMismatch`.

### Variadic functions

There are two ways to accept a variable number of arguments.

**Dotted rest.** A `.` before the last parameter collects all extra arguments
into a list bound to that name:

```clojure
(fn log (level . args)
  (str-join (fmap to-str (cons level args)) " "))
(log "info" "x =" 42)   ;; => "info x = 42"
```

Here `level` is required and `args` gets everything else (as a cons list, `()`
if there were no extra arguments). Calling with fewer than the required
positional count is still an `ArityMismatch`.

**Bare-ident params.** If the parameter "list" is a single identifier rather
than a parenthesized list, *all* arguments are bundled into it:

```clojure
(fn sum xs (reduce + 0 xs))
(sum 1 2 3 4)      ;; => 10
```

This is shorthand for "zero positional parameters, everything goes to the rest."

### Anonymous functions

Omit the name to get an anonymous closure. It is not bound anywhere and cannot
refer to itself by name:

```clojure
((fn (x) (* x x)) 6)            ;; => 36
(let inc (fn (x) (+ x 1)))      ;; bind it yourself if you like
(inc 4)                         ;; => 5
```

By convention, anonymous closures are often written with a throwaway name like
`_` when a name slot reads more clearly:

```clojure
(fmap (fn _ (x) (* x 2)) [1 2 3])   ;; => [2 4 6]
```

> **Disambiguation.** A three-element `fn` form is read by whether its middle
> item is a `(doc ...)` form. `(fn xs (a b) body)` is *named* — `xs` is the
> name, `(a b)` the params. `(fn xs (doc "…") body)` is *anonymous with a doc
> slot*. When in doubt, give the params explicit parentheses.

### Recursion

A named function is bound under its own name *inside its body*, which is what
lets it call itself:

```clojure
(fn fact (n)
  (if (< n 1) 1
    (* n (fact (- n 1)))))
(fact 5)           ;; => 120
```

Anonymous functions have no name to recurse through — use a named `fn` when you
need recursion.

> **No tail-call optimization.** Deep recursion consumes call-frame depth and is
> capped (10,000 nested evaluations by default) to protect the host. For
> iteration over large sequences, prefer `reduce`, `for`, or `loop`, which do
> not recurse. See [Performance](../idioms/performance.md).

## Closures capture their environment

A closure created by `fn` captures a **snapshot** of the environment at the
point of definition. Later rebinding of an outer name does not affect it (see
[Evaluation](evaluation.md) for the full story). To share mutable state into a
closure, capture a [ref](refs.md):

```clojure
(let c (ref 0))
(fn bump () (set! c (+ (deref c) 1)))
(bump) (bump) (bump)
(deref c)          ;; => 3
```

The closure captured the *cell* `c`, so every call mutates the same counter.

---

*See also:* [Special Forms](special-forms.md) · [Refs and Mutability](refs.md) ·
[Control Flow](control-flow.md) · [The Standard Library](stdlib.md) ·
*SPEC.md* §5–§6
