# Errors and Exceptions

rizz gives you three distinct ways to deal with failure, and choosing the right
one is a real part of writing good rizz. From least to most ceremony:

1. **Structural faults** — bugs. They always abort; you cannot catch them.
2. **Errors as values** — expected outcomes encoded as ordinary return values.
3. **Exceptions** — a deliberate unwind across many call frames.

## Structural faults are not catchable

An unbound identifier, a call to a non-callable, the wrong number or type of
arguments, an integer overflow or divide-by-zero, a NaN comparison, a failed
module load — these are **structural faults**. They abort the program and report
where they happened. rizz has no condition system and no way to resume; a bug is
a bug.

```clojure
(+ 1 "two")    ;; TypeMismatch — aborts
nope           ;; UnknownIdent — aborts
(car 5)        ;; TypeMismatch — car wants a cons
```

Crucially, [`try`](#exceptions) does **not** catch these. That is by design: a
`try` can never silently swallow a genuine bug. Only values you raise on purpose
are catchable.

## Errors as values

For _expected_ failures — a parse that might not succeed, a lookup that might
miss — return a value that encodes the outcome. A symbol makes a convenient
**tag**, and a two-element list pairs a tag with its payload:

```clojure
(fn parse (s)
  (let n (str->int s))
  (if (= n ()) '(err "not a number")
               `(ok ,n)))

(let result (parse "42"))
(if (= (car result) 'ok)
    (car (cdr result))      ;; => 42
    "handle failure")
```

The `ok` / `err` convention is just taste — `some` / `none` or any other tags
work equally well. The point is that the caller inspects the tag with `=` and
branches with `if` or [`cond`](control-flow.md). Many standard functions already
use this idea: `get` and `str->int` return `()` on a miss, which is falsy and
slots straight into `if`.

> **Gotcha — tags inside quasiquote.** Notice the tag is a _bare_ `ok`, not
> `'ok`, inside the `` ` `` template. Within a quasiquote a leading quote is
> kept literally, so `` `('ok ,n) `` would build the list `((quote ok) 42)` —
> whose head is `(quote ok)`, not the symbol `ok` — and `(= (car result) 'ok)`
> would then be false. Write the tag bare in a quasiquote (`` `(ok ,n) ``); use
> `'ok` only when you're _reading_ the tag back out, as in the `=` test above.

This is the idiom to prefer for ordinary control flow. It keeps failure explicit
and local, with no hidden unwinding.

## Exceptions

When a failure needs to unwind across _many_ frames — past functions that have no
business handling it — reach for the exception system. It is layered on the
errors-as-values idea: an exception is just a tagged cons `('Name arg...)`, and
`raise` carries one up the stack until a `try` catches it.

| Form / function               | Role                                                |
| ----------------------------- | --------------------------------------------------- |
| `(exception NAME)`            | Bind `NAME` to a constructor.                       |
| `(raise V)`                   | Abort evaluation, raising `V` to the nearest `try`. |
| `(try BODY (catch VAR H...))` | Catch a raised value, bind it to `VAR`.             |
| `(try-with BODY CLAUSES...)`  | Catch and dispatch by constructor (prelude macro).  |
| `(exn? TAG E)`                | True iff `E` is a cons tagged `TAG`.                |
| `(failwith MSG)`              | Raise the standard `('Failure MSG)`.                |

### Declaring and raising

`exception` binds a name to a constructor — a variadic function that builds a
tagged cons:

```clojure
(exception Not-found)
(exception Bad-input)

(Bad-input "x")    ;; => ('Bad-input "x")
(raise (Bad-input "x"))   ;; unwinds to the nearest try
```

`raise` accepts _any_ value, not only constructor-built ones — the tagged-cons
shape is convention, not enforcement. An uncaught `raise` aborts the program
like any structural fault.

### Catching with `try`

```clojure
(try BODY
  (catch VAR HANDLER...))
```

`try` evaluates `BODY`. If it raises, the raised value is bound to `VAR` and the
`catch` handler runs in its place; otherwise `try` returns the body's value. An
optional `(finally CLEANUP...)` clause runs on _every_ exit path — normal
return, caught raise, or re-propagated error:

```clojure
(exception Bad-input)
(try (raise (Bad-input "x"))
  (catch e (if (exn? 'Bad-input e) (car (cdr e)) (raise e))))   ;; => "x"
```

`exn?` tests a value's tag, so the handler can decide whether this is its
exception and re-`raise` if not. `try` is _not_ a scope escape hatch — bindings
made inside `BODY` or a handler do not leak out.

### `try-with` — match by constructor

Writing `exn?` chains by hand is tedious, so the prelude provides `try-with`, an
OCaml-style handler that matches on the constructor and binds the payload:

```clojure
(exception Not-found)
(exception Bad-input)

(try-with (lookup k)
  (with (Not-found)     'missing)                 ;; match by constructor
  (with (Bad-input msg) (concat "bad: " msg))     ;; bind the payload to msg
  (else  e              (raise e)))               ;; catch-all; re-raise
```

Each `(with (CTOR PARAMS...) HANDLER...)` clause compares the raised value's tag,
and on a match binds `PARAMS` to the payload elements in order. A trailing
`(else VAR HANDLER...)` matches anything; with no `else`, an unmatched exception
is re-raised. `failwith` is a shorthand that raises the standard `'Failure`
exception:

```clojure
(try-with (failwith "boom")
  (with (Failure m) m))     ;; => "boom"
```

Reach for the low-level `try`/`catch` when you want full control over the raw
value, and `try-with` for the common "dispatch by constructor" case.

### The module boundary

An exception raised while a module is being [loaded](modules.md) surfaces wrapped
as an `InModule` error and is **not** caught by a `try` in the importing file.
Catching is limited to a raise within the same evaluation.

## Choosing among the three

- **Expected, local outcome?** Errors as values (`'ok`/`'err`, or `()` for a
  miss). This should be your default.
- **Must unwind across many frames?** An exception with `try-with`.
- **A genuine bug?** Let the structural fault abort — don't try to paper over it.

[Idioms](../idioms/idioms.md) revisits this choice with larger examples.

---

_See also:_ [Collections](collections.md) · [Modules](modules.md) ·
[Control Flow](control-flow.md) · [Idioms](../idioms/idioms.md) ·
_SPEC.md_ §12
