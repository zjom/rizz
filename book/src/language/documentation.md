# Documentation

rizz lets you attach documentation to a binding, and read it back with `show`.
The whole standard library is documented this way — `(show cond)` prints the
docs for `cond` — and you can document your own functions and macros the same
way.

## The `doc` slot

The binding forms `let`, `let!`, `fn`, and `defmacro` accept an optional
`(doc ...)` slot:

```clojure
(let  NAME    (doc ARG+) VALUE)
(let! NAME    (doc ARG+) VALUE)
(fn   NAME PARAMS (doc ARG+) BODY)
(defmacro NAME PARAMS (doc ARG+) BODY)
```

A documented function looks like this — the `doc` form sits at the front of the
body:

```clojure
(fn inc (n)
  (doc "increments a number by 1"
       "params: `n` int"
       "returns: int")
  (+ n 1))

(show inc)
;; => "increments a number by 1\nparams: `n` int\nreturns: int"

(inc 4)        ;; => 5   — the doc form does not affect evaluation
```

The `doc` form takes one or more arguments. Each is **evaluated** in the
surrounding scope and must produce either a string or a collection (array or
list) of strings; collections are flattened recursively. All the collected
strings are joined with `\n`. That means doc text can be a literal, pulled from
a variable, or assembled from fragments:

```clojure
(let header "increments a number by 1")
(let lines ["params: `n` int" "returns: int"])
(fn inc (n) (doc header lines) (+ n 1))
(show inc)
;; => "increments a number by 1\nparams: `n` int\nreturns: int"
```

## `show`

`show` returns the doc string attached to its argument, or `()` if there is
none. [Refs](refs.md) are peeled, so `(show r)` and `(show (deref r))` are the
same when `r` holds a callable:

```clojure
(fn bare (n) (+ n 1))
(show bare)        ;; => ()   — no doc was attached

(let plain 42)
(show plain)       ;; => ()   — non-callables have no doc slot
```

## Where the doc lives

Documentation attaches to the **value**, specifically to closures, macros, and
native functions. For `let` / `let!`, if the bound value is not a callable — an
int, a string, a collection — the doc is silently dropped, because non-callables
have nowhere to store it. So document functions and macros; for data, a comment
is the right tool.

## `doc` is context-sensitive

`doc` is special *only* as the head of the documentation slot inside a binding
form. It is not a general special form, and it is not in the reserved-identifier
list. Anywhere else, `(doc ...)` is read as an ordinary function call — which
fails with `UnknownIdent("doc")` unless you have bound `doc` to a callable
yourself. A `doc` form with zero arguments is an `ArityMismatch`, and a non-string,
non-collection argument is a `TypeMismatch`.

In practice this never bites: put the `(doc ...)` form right where the grammar
expects it (immediately after the params in a `fn`/`defmacro`, or before the
value in a `let`/`let!`), and it just works.

## A convention for good docs

The prelude's own docs follow a readable shape worth imitating: a one-line
signature, a blank line, a short description, an example, and a "See also" line.
You can see it across `src/prelude/_.rz`:

```clojure
(defmacro unless (cond . body)
  (doc "(unless COND BODY...)"
       ""
       "Evaluates BODY when COND is falsy, returning the value of the last"
       "form. Returns () when COND is truthy."
       ""
       "Example:"
       "  (unless (> 1 2) 'ok)  ;; => ok"
       ""
       "See also: (if COND THEN ELSE).")
  `(if ,cond () (do ,@body)))
```

Each line is a separate string argument, so the joined result reads as tidy
multi-line help when surfaced by `show`.

---

*See also:* [Bindings and Functions](functions.md) ·
[Macros and Metaprogramming](macros.md) · [The Standard Library](stdlib.md) ·
*SPEC.md* §11
