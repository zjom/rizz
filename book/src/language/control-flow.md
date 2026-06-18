# Control Flow

Beyond the `if` special form, rizz's control-flow constructs — `cond`, `match`,
`unless`, `for`, `loop`, `while`, `and`, `or` — are **macros defined in the
prelude**, written in rizz itself. That has two practical consequences:

- They behave like syntax (e.g. `and`/`or` short-circuit, `for` binds a name),
  but they are *ordinary bindings*, not reserved keywords. You can shadow them.
- Because they expand to `if`, `reduce`, and closures, they inherit those
  semantics — including the absence of a built-in accumulator in the looping
  forms (use a [ref](refs.md)).

## `cond` — multi-way conditional

```clojure
(cond (TEST BODY) ... (else BODY))
```

`cond` walks the clauses top to bottom and returns the `BODY` of the first
clause whose `TEST` is truthy. A literal `else` always matches. With no matching
clause and no `else`, the result is `()`.

```clojure
(let x 0)
(cond ((< x 0) 'neg)
      ((= x 0) 'zero)
      (else    'pos))     ;; => zero
```

## `match` — dispatch on a value through predicates

```clojure
(match VAL (PRED EXPR) ... (else EXPR))
```

`match` evaluates `VAL` once, then tries each clause. The clause's predicate is a
*call form* with the matched value inserted as its **first argument** — so
`(< 10)` becomes `(< VAL 10)`. The first truthy predicate wins.

```clojure
(match 3 ((< 10) 'small) (else 'big))   ;; => small   — tests (< 3 10)

(let v [1 2])
(match v ((is 'map)   'mapish)
         ((is 'array) 'arrayish)
         (else        'other))          ;; => arrayish
```

`match` pairs beautifully with [`is`](stdlib.md), which returns its argument
when the type matches (truthy) and `()` otherwise.

## `unless` — inverted conditional

```clojure
(unless COND BODY...)
```

Evaluates the body when `COND` is **falsy**, returning the last form's value;
otherwise `()`. It is exactly `(if COND () (do BODY...))`:

```clojure
(unless (> 1 2) 'ok)   ;; => ok
(unless (< 1 2) 'ok)   ;; => ()
```

## `for` — iterate a sequence

```clojure
(for VAR SEQ BODY...)
```

Binds each element of `SEQ` to `VAR` and runs the body. It accepts anything
[`reduce`](collections.md) accepts — strings, arrays, maps, and lists. Because
`for` is built on `reduce`, it provides **no accumulator**: use a ref when you
need to build a result.

```clojure
(let! sum 0)
(for x [1 2 3 4] (set! sum (+ (deref sum) x)))
(deref sum)        ;; => 10
```

## `loop` — repeat N times

```clojure
(loop N BODY...)
```

Runs the body `N` times. Inside the body, `__i` is bound to the current index
(`0 .. N`). Returns `()` when `N ≤ 0`.

```clojure
(let! acc 0)
(loop 5 (set! acc (+ (deref acc) __i)))
(deref acc)        ;; => 10   — 0 + 1 + 2 + 3 + 4
```

## `while` — repeat while truthy

```clojure
(while COND BODY...)
```

Re-checks `COND` before each iteration and runs the body while it stays truthy,
returning the body's most recent value (or `()` if it never ran).

```clojure
(let! i 0)
(let! sum 0)
(while (< (deref i) 5)
  (set! sum (+ (deref sum) (deref i)))
  (set! i (+ (deref i) 1)))
(deref sum)        ;; => 10
```

> `while` is a *recursive* macro, so a `while` loop that runs an enormous number
> of times can hit the recursion limit. For iterating a known sequence, prefer
> `for` or `reduce`, which do not recurse. See
> [Performance](../idioms/performance.md).

## `and`, `or` — short-circuit logic

```clojure
(and A B)
(or A B)
```

These follow Lua-style *value* semantics rather than returning a boolean:

- `or` returns `A` if `A` is truthy, otherwise `B`.
- `and` returns `B` if `A` is truthy, otherwise `A`.

`A` is evaluated once; `B` only when needed.

```clojure
(or 5 9)          ;; => 5
(or 0 9)          ;; => 9
(or () 42)        ;; => 42
(and 1 2)         ;; => 2
(and 0 9)         ;; => 0
(and () (/ 1 0))  ;; => ()   — RHS never evaluated, so no divide-by-zero
```

The short-circuiting makes them handy as guards: `(and (is x 'map) (get x k))`
only does the lookup when `x` really is a map.

---

*See also:* [Special Forms](special-forms.md) · [Refs and Mutability](refs.md) ·
[Collections](collections.md) · [Macros and Metaprogramming](macros.md) ·
*SPEC.md* §9
