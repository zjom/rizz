# Collections

rizz has three built-in collection kinds — **arrays**, **maps**, and **cons
lists** — plus **strings**, which behave like sequences of characters. A large
family of functions works _polymorphically_ across all of them, so once you
learn `len`, `get`, `fmap`, and `reduce`, you know them for every collection.

All collections are **persistent**: operations like `push` and `put` return a
new, structurally-shared collection instead of mutating the original. For
in-place mutation you commit a result into a [ref](refs.md) with the `!`-suffixed
operations.

## The three collection kinds (and strings)

```clojure
[1 2 3]                  ;; array  — persistent vector, integer-indexed
{ "a" : 1  "b" : 2 }     ;; map    — persistent hash map, any-value keys
(cons 1 (cons 2 ()))     ;; cons   — linked list; (1 2) when quoted
"hello"                  ;; str    — sequence of characters
```

A bare non-cons value is treated by the iteration helpers as a one-element
sequence containing itself, which is occasionally convenient.

## Polymorphic operations

These work uniformly across strings, arrays, maps, and cons lists.

| Operation             | Works on       | Description                                              |
| --------------------- | -------------- | -------------------------------------------------------- |
| `(len c)`             | all            | Element count (string by character).                     |
| `(get c k)`           | all            | Index/key lookup; miss → `()`.                           |
| `(contains? c x)`     | all            | Substring / element / key test.                          |
| `(concat a b)`        | matching kinds | Join two of the same kind (right map wins on key clash). |
| `(fmap f c)`          | all            | Map `f` over elements, returning the same kind.          |
| `(fmapi f c)`         | all            | Like `fmap`, but `f` also receives the index.            |
| `(filter pred c)`     | all            | Keep elements where `pred` is truthy.                    |
| `(reduce f init c)`   | all            | Left fold from `init`.                                   |
| `(all pred c)`        | all            | True if `pred` holds for every element.                  |
| `(any pred c)`        | all            | True if `pred` holds for any element.                    |
| `(zip a b)`           | all            | List of pairs; length is `min(len a, len b)`.            |
| `(slice c start end)` | str/array/list | Half-open `[start, end)`, clamped to bounds.             |
| `(reverse c)`         | str/array/list | Reversed copy.                                           |
| `(first c)`           | str/array/list | Head, or `()` if empty.                                  |
| `(last c)`            | str/array/list | Last element, or `()` if empty.                          |
| `(rest c)`            | str/array/list | Everything but the first.                                |
| `(find pred c)`       | str/array/list | Index of the first match, or `()`.                       |

```clojure
(len [1 2 3])              ;; => 3
(len "héllo")             ;; => 5   — counts characters, not bytes
(get [10 20 30] 1)        ;; => 20
(get { "x" : 9 } "x")     ;; => 9
(get [1 2 3] 99)          ;; => ()  — out of range is a miss, not an error
(contains? "abc" "b")     ;; => 1
(slice [0 1 2 3 4] 1 3)   ;; => [1 2]
(reverse "abc")           ;; => "cba"
(find (fn _ (x) (> x 1)) [0 1 2 3])   ;; => 2
```

### `fmap`, `filter`, `reduce`

These three are the backbone of functional rizz.

```clojure
(fmap (fn _ (x) (* x x)) [1 2 3])         ;; => [1 4 9]
(filter (fn _ (x) (> x 2)) [1 2 3 4])     ;; => [3 4]
(reduce + 0 [1 2 3 4])                    ;; => 10
(reduce (fn _ (acc x) (cons x acc)) () [1 2 3])   ;; => (3 2 1)
```

**Map callbacks have a special shape.** When the collection is a map, the
callback receives the key and value (and index, for `fmapi`):

```clojure
;; fmap over a map: f takes (k v) and returns [k' v']
(fmap (fn _ (k v) [k (* v 10)]) { "a" : 1  "b" : 2 })
;; => { "a" : 10  "b" : 20 }

;; filter over a map: pred takes (k v)
(filter (fn _ (k v) (> v 1)) { "a" : 1  "b" : 2 })
;; => { "b" : 2 }

;; reduce over a map: f takes (acc k v)
(reduce (fn _ (acc k v) (+ acc v)) 0 { "a" : 1  "b" : 2 })
;; => 3
```

`fmapi` adds an index as the first callback argument (`(i x)` for sequences,
`(i k v)` for maps):

```clojure
(fmapi (fn _ (i x) [i x]) ["a" "b"])   ;; => [[0 "a"] [1 "b"]]
```

## Arrays

Integer-indexed persistent vectors.

| Operation            | Description                                       |
| -------------------- | ------------------------------------------------- |
| `(push xs x)`        | New array with `x` appended.                      |
| `(pop xs)`           | New array without the last element.               |
| `(array-set xs i x)` | New array with index `i` replaced.                |
| `(range start end)`  | Array of ints in `[start, end)`.                  |
| `(array-of x)`       | Single-element array `[x]`.                       |
| `(array-from xs)`    | Build an array from `xs` (traverses if iterable). |

```clojure
(push [1 2] 3)         ;; => [1 2 3]
(pop [1 2 3])          ;; => [1 2]
(array-set [1 2 3] 1 99)   ;; => [1 99 3]
(range 0 5)            ;; => [0 1 2 3 4]
(array-from (range 1 4))   ;; => [1 2 3]
```

The in-place variants `push!`, `pop!`, `array-set!` operate on a
[ref](refs.md)-of-array; see that chapter.

## Maps

Persistent hash maps. Keys may be any value; insertion order is not preserved.

| Operation     | Description                                 |
| ------------- | ------------------------------------------- |
| `(put m k v)` | New map with `k → v` inserted.              |
| `(del m k)`   | New map with `k` removed (no-op if absent). |
| `(keys m)`    | Array of keys (unspecified order).          |
| `(values m)`  | Array of values (unspecified order).        |

```clojure
(put { "a" : 1 } "b" 2)        ;; => { "a" : 1  "b" : 2 }
(del { "a" : 1  "b" : 2 } "a") ;; => { "b" : 2 }
(get { "a" : 1 } "a")          ;; => 1
(keys { 1 : "one"  2 : "two" })   ;; => array of the keys
```

In-place: `put!`, `del!` on a ref-of-map.

## Cons lists

The classic Lisp pair. A list is a chain of cons cells ending in `()`.

| Operation    | Description                            |
| ------------ | -------------------------------------- |
| `(cons h t)` | A new cell with head `h` and tail `t`. |
| `(car xs)`   | The head. `(car ())` is `()`.          |
| `(cdr xs)`   | The tail. `(cdr ())` is `()`.          |

```clojure
(cons 1 (cons 2 (cons 3 ())))   ;; => (1 2 3)
(car '(1 2 3))                  ;; => 1
(cdr '(1 2 3))                  ;; => (2 3)
```

`tail` is usually a list, but any value is allowed — `(cons 1 2)` is an improper
pair. In-place head/tail replacement uses `car!`/`cdr!` on a ref-of-cons.

Tagged cons cells — a list whose head is a symbol, like `(ok value)` or
`(err message)` — are the conventional way to encode
[errors as values](errors.md).

## Choosing a collection

- **Array** — ordered, index-addressable, the default sequence. Reach for it
  unless you have a reason not to.
- **Map** — keyed lookup by arbitrary keys.
- **Cons list** — building up sequentially from the front, pattern-style
  recursion, and tagged data (`'ok`/`'err`). It is also what `quote` and macros
  hand you.

---

_See also:_ [Values and Types](values.md) · [Refs and Mutability](refs.md) ·
[The Standard Library](stdlib.md) · [Errors and Exceptions](errors.md) ·
_SPEC.md_ §10
