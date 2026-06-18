# Idioms

This chapter collects the patterns that show up again and again in good rizz
code. None of it is new language — it is the standard library and evaluation
model used the way they were meant to be used.

## Prefer data pipelines to loops

Most "loops" are really transformations of a sequence, and rizz expresses those
directly with `fmap`, `filter`, and `reduce`. The result is shorter and avoids
the ceremony of a [ref](../language/refs.md) accumulator:

```clojure
;; sum of squares of the evens in 0..10
(reduce + 0
  (fmap (fn _ (x) (* x x))
    (filter (fn _ (x) (= (mod x 2) 0))
      (range 0 10))))
;; => 120
```

When a pipeline gets deep, `pipe` reads top-to-bottom and names the stages:

```clojure
(let process
  (pipe (fn _ (xs) (filter (fn _ (x) (> x 0)) xs))
        (fn _ (xs) (fmap (fn _ (x) (* x 2)) xs))
        (fn _ (xs) (reduce + 0 xs))))
(process [-1 2 -3 4])   ;; => 12
```

Reserve explicit `for`/`while`/`loop` for genuine side effects (printing,
mutating a ref), not for building values.

## Build small combinators

The [function combinators](../language/stdlib.md) — `partial`, `flip`,
`complement`, `compose`, `on` — let you name a behavior once and reuse it:

```clojure
(let positive? (fn _ (x) (> x 0)))
(let non-positive? (complement positive?))
(filter non-positive? [-1 0 1 2])     ;; => [-1 0]

(let by-length (on < len))            ;; compare two values by their length
(by-length "a" "bbb")                 ;; => 1
```

`partial` and `flip` turn a general function into a specialized one without a
`fn` wrapper:

```clojure
(let inc   (partial + 1))
(let halve (partial (flip /) 2))
(fmap inc [1 2 3])     ;; => [2 3 4]
(halve 10)             ;; => 5
```

## Errors as values first, exceptions when they pay off

Default to returning a tagged value for expected failures; it keeps the failure
visible and local:

```clojure
(fn safe-div (a b)
  (if (= b 0) '(err "divide by zero")
              `(ok ,(/ a b))))

(match (safe-div 10 2)
  ((= '(ok 5)) "got five")
  (else        "something else"))
```

(Keep the tag _bare_ inside the quasiquote — `` `(ok ,x) ``, not `` `('ok ,x) ``
— so the list's head is the symbol `ok`. See the gotcha in
[Errors and Exceptions](../language/errors.md).)

Save [exceptions](../language/errors.md) for when an error must travel up
through several layers that have nothing useful to say about it. `try-with`
keeps the catch site readable:

```clojure
(exception Not-found)
(try-with (deep-lookup db key)
  (with (Not-found) (default-for key))
  (else e (raise e)))
```

A useful rule: if the immediate caller can sensibly handle the failure, return a
value. If the handler is many frames away, raise.

## Variadic helpers with rest parameters

Dotted-rest and bare-ident parameters make clean variadic APIs. A logger that
takes a level plus any number of parts:

```clojure
(fn log (level . parts)
  (str-join (fmap to-str (cons level parts)) " "))
(log "info" "x =" 42)     ;; => "info x = 42"
```

A fully-variadic reducer:

```clojure
(fn sum xs (reduce + 0 xs))
(sum 1 2 3 4)             ;; => 10
```

## Tagged cons cells for structured data

A symbol as a leading tag is the idiomatic lightweight "struct" — write it bare
inside the quasiquote so the head is the symbol itself:

```clojure
(fn make-point (x y) `(point ,x ,y))
(fn point-x (p) (car (cdr p)))
(fn point-y (p) (car (cdr (cdr p))))

(let p (make-point 3 4))
(point-x p)              ;; => 3
(exn? 'point p)          ;; => 1   — the same tag-test used for exceptions
```

For anything with named fields, a [map](../language/collections.md) is usually
clearer than positional cons access.

## Namespace modules you import broadly

`(open "mod")` dumps every binding into your scope, including `_`-prefixed ones.
When a module is large or its names are generic, import it under a prefix so the
provenance stays obvious and nothing collides:

```clojure
(open "geometry" geo)
(geo.area (geo.circle 2))
```

Use the bare `(open "mod")` form for small, tightly-related helpers where the
extra qualification would just be noise.

## Document the things you `show`

Because `show` reads docs off callables, a documented function is
self-describing at the REPL. Give your public functions a `(doc ...)` slot with a
signature line and a short example, following the prelude's house style
([Documentation](../language/documentation.md)). It costs little and makes a
library pleasant to explore interactively.

---

_See also:_ [Patterns and Anti-patterns](patterns.md) ·
[Performance](performance.md) · [The Standard Library](../language/stdlib.md) ·
_SPEC.md_ §14
