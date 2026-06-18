# Refs and Mutability

rizz is mostly value-oriented: bindings produce new environments, collections
are persistent, and calls don't leak bindings. The one exception is the
**ref** — a heap cell whose contents you can replace in place. Refs are the
*only* path to mutation; everything else stays immutable.

If you find yourself wanting a variable you can reassign, a counter, an
accumulator, or shared state between calls, you want a ref.

## The core operations

| Operation     | Arity | What it does                                            |
| ------------- | ----- | ------------------------------------------------------- |
| `(ref v)`     | 1     | Allocate a new cell initialized to `v`.                 |
| `(deref r)`   | 1     | Read the cell's current contents.                       |
| `(set! r v)`  | 2     | Store `v` in the cell; returns the new value.           |
| `(let! n v)`  | —     | Special form: `(let n (ref v))`. See [Functions](functions.md). |

```clojure
(let! c 0)
(set! c (+ (deref c) 1))
(set! c (+ (deref c) 1))
(deref c)          ;; => 2
```

A ref is a shared cell. Two bindings of the same ref point at the same storage,
so a write through one is visible through the other:

```clojure
(let a (ref 1))
(let b a)          ;; b and a alias the same cell
(set! b 99)
(deref a)          ;; => 99
```

Closures that capture a ref capture the **cell**, not a snapshot of its
contents — which is how you carry mutable state across the call boundary that
otherwise isolates scopes:

```clojure
(let c (ref 0))
(fn bump () (set! c (+ (deref c) 1)))
(bump) (bump)
(deref c)          ;; => 2
```

## In-place collection operations

The `!`-suffixed operations take a ref whose cell holds a particular kind of
collection, mutate it, and return the new contents. They error if the first
argument isn't a ref, or if the cell doesn't hold the expected type.

| Operation              | Cell holds | Effect                            |
| ---------------------- | ---------- | --------------------------------- |
| `(push! r x)`          | array      | Append `x`.                       |
| `(pop! r)`             | array      | Remove the last element.          |
| `(array-set! r i x)`   | array      | Replace the element at index `i`. |
| `(put! r k v)`         | map        | Insert `k → v`.                   |
| `(del! r k)`           | map        | Remove key `k` (no-op if absent). |
| `(car! r x)`           | cons       | Replace the head; keep the tail.  |
| `(cdr! r x)`           | cons       | Replace the tail; keep the head.  |

```clojure
(let! xs [1 2 3])
(push! xs 4)
(deref xs)         ;; => [1 2 3 4]
(pop! xs)
(deref xs)         ;; => [1 2 3]
```

These commit a result back into a cell. For *non-mutating* updates, use the
unsuffixed forms (`push`, `pop`, `put`, `del`, `cons`), which return a new
collection and leave the original alone — see [Collections](collections.md).

## Where refs are auto-peeled

Most operations treat a ref as opaque: you must `deref` to see inside. A small,
fixed set of contexts look *through* a ref automatically. Know this list,
because everything *not* on it does not peel:

| Context                     | Behavior                                                            |
| --------------------------- | ------------------------------------------------------------------ |
| Truthiness tests            | A ref is truthy iff its contents are. `(if (ref 0) ...)` → else.    |
| Numeric ops & comparisons   | `(+ (ref 5) 1)` → `6`; `<`, `>=`, etc. read through to a number.    |
| Head position of a call     | A ref-of-callable dispatches as the callable.                      |
| `show`                      | Reads the doc through a ref-of-callable.                           |
| `empty-of`                  | Returns the zero of the *contents*, not a fresh ref.               |

Everything else — equality, `typeof`, collection access, `=` — treats the ref as
itself.

```clojure
(typeof (ref 5))         ;; => ref     — NOT int
(typeof (deref (ref 5))) ;; => int
```

## Footguns

Refs are the one place rizz's value-oriented intuitions break down. Four traps:

1. **Equality is by cell identity, not contents.**

   ```clojure
   (= (ref 5) (ref 5))   ;; => 0   — two different cells
   ```

2. **No auto-collapse on construction.** `ref`, `set!`, `push!`, `put!`, and the
   rest store *exactly* what you hand them. Wrapping a ref in a ref gives you two
   layers, and both must be `deref`d:

   ```clojure
   (deref (deref (ref (ref 7))))   ;; => 7
   ```

3. **`typeof` on a ref returns `ref`**, not the inner type. Use `typeof` on a
   `deref` if you want the contents' kind.

4. **A ref holding a non-callable still errors in head position.** Only callable
   contents auto-peel; a ref holding `5` in head position is `NotCallable`.

A good rule of thumb: keep refs local and explicit. Mutate through a clearly
named `let!` binding, read with `deref`, and prefer returning fresh values over
threading refs through deep call chains. [Patterns and
Anti-patterns](../idioms/patterns.md) has more on when a ref earns its keep.

---

*See also:* [Values and Types](values.md) · [Collections](collections.md) ·
[Control Flow](control-flow.md) · [Performance](../idioms/performance.md) ·
*SPEC.md* §7
