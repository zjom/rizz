# Values and Types

Every rizz expression evaluates to a **value**. There are a fixed number of
value kinds, and `typeof` will tell you which one you have:

```clojure
(typeof 42)        ;; => int
(typeof 3.14)      ;; => float
(typeof "hi")      ;; => str
(typeof [1 2 3])   ;; => array
(typeof {})        ;; => map
```

## The value universe

| Kind        | What it is                              | Literal / constructor |
| ----------- | --------------------------------------- | --------------------- |
| `int`       | 64-bit signed integer                   | `42`, `-7`            |
| `float`     | 64-bit IEEE-754 float                   | `3.14`, `1.`          |
| `str`       | UTF-8 string                            | `"hi"`                |
| `ident`     | Interned identifier (only via `quote`)  | `'foo`                |
| `unit`      | nil — also the empty list               | `()`                  |
| `cons`      | A linked-list cell (head + tail)        | `(cons 1 ())`         |
| `array`     | Persistent vector                       | `[1 2 3]`             |
| `map`       | Persistent hash map                     | `{ "k" : 1 }`         |
| `closure`   | A user-defined function                 | `(fn _ (x) x)`        |
| `native-fn` | A builtin function                      | `+`, `len`            |
| `ref`       | A mutable cell — the only mutable value | `(ref 0)`             |

A couple of these deserve a note now; the rest get their own chapters.

**Identifiers as values.** You normally only see an `ident` value when you
[quote](special-forms.md) a name: `'foo` is the identifier `foo` as data.
Unquoted, `foo` is looked up in the environment, not treated as a value.

**Lists are cons chains.** A list like `(1 2 3)` is three `cons` cells ending in
`unit`. `()` is `unit`. This matters because rizz iteration helpers treat a
_non-cons_ value as a one-element sequence containing itself — useful, and
occasionally surprising.

## Truthiness

rizz has **no boolean type**. Conditionals (`if`, `not`, `and`, `or`, `cond`,
…) ask whether a value is _truthy_. The following values are **false**:

- `()` — unit / nil
- `0` and `0.0` — integer and float zero
- `""` — the empty string
- the empty identifier
- `[]` — the empty array
- `{}` — the empty map

**Everything else is true** — non-zero numbers, non-empty strings and
collections, and _all_ functions.

```clojure
(if 0 'yes 'no)       ;; => no
(if "" 'yes 'no)      ;; => no
(if [] 'yes 'no)      ;; => no
(if [0] 'yes 'no)     ;; => yes   — non-empty array, even of a falsy element
(if -1 'yes 'no)      ;; => yes   — non-zero
```

Because functions are always truthy and operations return values, the standard
library uses **`1` for true and `0` for false**:

```clojure
(= 1 1)               ;; => 1
(< 2 1)               ;; => 0
(contains? "abc" "b") ;; => 1
```

A [`ref`](refs.md) is truthy exactly when its _contents_ are truthy — this is
one of the few places a ref is transparently "seen through":

```clojure
(if (ref 0) 'yes 'no) ;; => no   — the ref holds 0
```

## Equality

`=` (also spelled `eq`) is **structural** equality. Two collections are equal
when their contents are equal, recursively:

```clojure
(= [1 2 3] [1 2 3])           ;; => 1
(= { "a" : 1 } { "a" : 1 })   ;; => 1
(= "ab" "ab")                 ;; => 1
```

Three subtleties:

- **Functions** compare by identity for native functions (same builtin) and
  structurally for closures (same name, params, body, captured env).
- **Refs compare by cell identity**, not contents. Two freshly-made refs holding
  the same value are _not_ equal:

  ```clojure
  (= (ref 5) (ref 5))   ;; => 0   — different cells
  (let r (ref 5))
  (= r r)               ;; => 1   — same cell
  ```

- **All NaN floats compare equal** to each other (a deliberate convenience so
  values stay usable as map keys).

Use `!=` (or `neq`) for the negation.

## Numbers: two kinds that never mix

`int` and `float` are distinct, and rizz performs **no implicit coercion**
between them. Arithmetic and comparison require both operands to be the same
kind:

```clojure
(+ 1 2)       ;; => 3
(+ 1.0 2.0)   ;; => 3.0
(+ 1 2.0)     ;; type error — int and float do not mix
```

To cross the boundary, convert explicitly:

```clojure
(+ 1 (int-of 2.0))     ;; => 3
(+ (float-of 1) 2.0)   ;; => 3.0
```

> **Display quirk.** A whole-number float prints the same as an int — the value
> `3.0` displays as `3`, and `(to-str 3.0)` is `"3"`. It is still genuinely a
> float: `(typeof 3.0)` is `float` and `(is 3.0 'float)` is truthy. Throughout
> this book a `;; => 3.0` annotation means "the float three" even though the CLI
> renders it `3`; reach for `typeof` when you need to tell the two kinds apart.

`int-of` rounds a float to the nearest integer (ties to even) and also parses
numeric strings; `float-of` converts an int (rounding if no exact float exists)
and parses strings. See the [Standard Library](stdlib.md).

### Fault policy differs by kind

This is a sharp edge worth internalizing:

- **Integer operations are checked.** Overflow and division by zero raise an
  `ArithmeticError` that aborts the program.

  ```clojure
  (/ 1 0)   ;; ArithmeticError — integer divide by zero
  ```

- **Float operations follow IEEE-754.** They never raise on their own:

  ```clojure
  (/ 1.0 0.0)   ;; => inf
  (/ 0.0 0.0)   ;; => NaN  (printed as NaN)
  ```

NaN propagates silently through float arithmetic, but it is _rejected_ wherever
an ordering is required: `cmp`, `min`, `max`, and `clamp` raise an
`ArithmeticError` when they meet a NaN, rather than producing nonsense.

### Numbers see through refs

The numeric operators (`+`, `-`, `<`, `>=`, …) transparently read through a ref
holding a number — the one read-through that happens without an explicit
`deref`:

```clojure
(+ (ref 5) 1)   ;; => 6
(< (ref 2) 3)   ;; => 1
```

This, truthiness, and a handful of other contexts are the _only_ places a ref is
auto-peeled. The full list is in [Refs and Mutability](refs.md).

---

_See also:_ [Refs and Mutability](refs.md) · [Collections](collections.md) ·
[The Standard Library](stdlib.md) · _SPEC.md_ §3
