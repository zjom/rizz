# Patterns and Anti-patterns

A catalog of "do this, not that," drawn from the sharp edges the earlier
chapters flagged. Each entry names the trap and the habit that avoids it.

## Stay value-oriented; reach for a ref deliberately

rizz wants you to compute new values, not mutate old ones. A [ref](../language/refs.md)
is the right tool for a counter, an accumulator inside a side-effecting loop, or
state shared across closure calls — and the wrong tool for "I'll just thread a
mutable bag through everything."

```clojure
;; Good — a ref where mutation is the actual point: a running counter
(let! hits 0)
(for x events (when (interesting? x) (set! hits (+ (deref hits) 1))))
(deref hits)

;; Prefer — no ref needed; reduce expresses the accumulation directly
(reduce (fn _ (n x) (if (interesting? x) (+ n 1) n)) 0 events)
```

If a function's signature starts sprouting refs so callers can observe its
internals, that is usually a sign the data should flow back as a return value
instead.

## Don't compare refs with `=`

[`=` on refs is identity](../language/refs.md), not contents. Comparing two refs
almost never does what you mean:

```clojure
(= (ref 5) (ref 5))            ;; => 0   — surprise!
(= (deref a) (deref b))        ;; compare contents — what you wanted
```

Deref first, then compare.

## `typeof` a ref's contents, not the ref

```clojure
(typeof (ref 5))               ;; => ref
(typeof (deref (ref 5)))       ;; => int   — usually what you want
```

The same applies to `is`: `(is (ref 5) 'int)` is `()`. Peel the ref when you
care about the inner kind.

## Quote vs. quasiquote

Use plain `quote` for fully-literal data, and `quasiquote` only when you need to
inject computed pieces. Reaching for `` ` `` when nothing is unquoted just adds
noise:

```clojure
'(a b c)                       ;; literal — quote
`(a ,(compute) c)              ;; one hole — quasiquote earns its keep
`(a b c)                       ;; pointless — should be '(a b c)
```

## Prefix macro temporaries with `__`

Macros are not hygienic. Any name your expansion binds can capture (or be
captured by) the caller's names. Follow the prelude convention and prefix
macro-internal names with `__`:

```clojure
;; Risky — `tmp` could collide with a caller's binding
(defmacro twice (x) `(do (let tmp ,x) (+ tmp tmp)))

;; Safe — __tmp is unlikely to clash
(defmacro twice (x) `(do (let __tmp ,x) (+ __tmp __tmp)))
```

See [Macros and Metaprogramming](../language/macros.md).

## Don't try to `catch` a bug

[`try` only catches `raise`d values](../language/errors.md). Structural faults —
`UnknownIdent`, `TypeMismatch`, `ArityMismatch`, arithmetic faults — blow past
`catch` on purpose. Don't wrap code in `try` hoping to suppress a type error;
fix the type error. Reserve exceptions for deliberate, recoverable unwinding.

## Mind the no-coercion rule at boundaries

Mixed int/float arithmetic is a `TypeMismatch`, and it most often bites at the
boundary between literal data and computed numbers:

```clojure
(* 2 3.5)                      ;; error — int times float
(* (float-of 2) 3.5)           ;; => 7.0
```

When a function might receive either kind, normalize at the entrance with
`int-of` / `float-of` rather than scattering conversions through the body.

## Reserved words can't be shadowed in head position

You can bind the _value_ of a name like `if`, but `(if ...)` in head position is
always the special form ([Evaluation](../language/evaluation.md)). Don't name a
function `if`, `let`, `fn`, `do`, `quote`, `eval`, `open`, `try`, or `exception`
and expect to call it — pick a non-reserved name. (The control-flow _macros_ like
`cond` and `while` are not reserved and _can_ be shadowed, but doing so is still
a good way to confuse a reader.)

## Let the falsy-miss convention work for you

`get`, `str->int`, `find`, and friends return `()` on a miss, and `()` is falsy.
Lean on that instead of separate "did it exist?" checks:

```clojure
(let v (get config "timeout"))
(if v v 30)                    ;; default when missing
;; or simply:
(or (get config "timeout") 30)
```

## Empty body? You probably want `do` or `unless`

`if` needs both branches. When you only have a "then," use `()` as the else or
reach for [`unless`](../language/control-flow.md)/`when`-style macros, and use
`do` to sequence multiple side effects into the single body slot a `fn`, `if`
branch, or `for` body expects.

---

_See also:_ [Idioms](idioms.md) · [Performance](performance.md) ·
[Refs and Mutability](../language/refs.md) ·
[Errors and Exceptions](../language/errors.md)
