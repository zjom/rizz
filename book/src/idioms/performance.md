# Performance

rizz is a tree-walking interpreter — it is built for embedding and scripting, not
for number-crunching. Still, a few facts about its execution model will keep you
out of trouble and help you write code that scales to large inputs.

## There is no tail-call optimization

This is the big one. A function that recurses does **not** reuse its stack frame,
so deep recursion accumulates call-frame depth. To keep runaway recursion from
crashing the host process, the evaluator caps nesting:

- A per-thread recursion limit (default **10,000** nested evaluations) raises a
  `RecursionLimit` error instead of overflowing the stack.
- The physical stack is grown in segments as depth increases, so legitimate deep
  recursion _within_ the limit won't overflow even on a small host thread.

The practical upshot: **don't iterate large sequences with self-recursion.**

```clojure
;; Risky on a long list — one frame per element, can hit the limit
(fn sum-rec (xs)
  (if (= xs ()) 0
    (+ (car xs) (sum-rec (cdr xs)))))

;; Better — reduce iterates without growing the stack
(fn sum (xs) (reduce + 0 xs))
```

`reduce`, `for`, and `loop` iterate _without_ recursion and are the right tools
for long sequences. Note that `while` is itself a **recursive macro**, so a
`while` that spins for a very large number of iterations is subject to the same
limit — prefer `for`/`reduce` when you are walking a known sequence.

Embedders can tune the cap; see [Error Handling](../embedding/errors.md) and
`rizz::runtime::set_recursion_limit`.

## Persistent data is cheap to copy, shared on update

Arrays and maps are **persistent** (backed by `im`'s structural-sharing
containers). Two consequences:

- **Cloning a value is cheap.** Passing collections around, returning them, and
  capturing them in closures does not deep-copy.
- **"Updates" share structure.** `push`, `put`, `del`, `array-set`, and the rest
  return a new collection that shares most of its internals with the original.
  The original stays valid and unchanged.

```clojure
(let a [1 2 3])
(let b (push a 4))     ;; b shares structure with a
a                      ;; => [1 2 3]   — untouched
b                      ;; => [1 2 3 4]
```

Because non-mutating updates are already cheap, you rarely _need_ the in-place
`!`-operations for performance — reach for them when you specifically want a
single cell to be observably mutated (a shared counter, an accumulator), not as a
blanket optimization.

## Persistent vs. in-place: a quick model

| Style                       | Cost                           | Use when                               |
| --------------------------- | ------------------------------ | -------------------------------------- |
| `push` / `put` (persistent) | new structure, shared interior | building values, functional pipelines  |
| `push!` / `put!` (in a ref) | mutate one cell in place       | a counter/accumulator you mutate often |

For a tight accumulation loop, a single `let!` ref mutated with `push!` avoids
allocating an intermediate collection per step. For everything else, the
persistent forms are clearer and plenty fast.

## Cons lists vs. arrays

- **Cons lists** are great for front-insertion (`cons` is O(1)) and head/tail
  recursion, but indexing (`get`) walks the spine.
- **Arrays** give indexed access and `len` cheaply, and are the better default
  for random access or when you mostly append.

When a function consumes a sequence linearly (`reduce`, `for`, `fmap`), either
works; choose by how the data is produced and accessed elsewhere.

## The prelude is built once per thread

Constructing the standard environment parses and evaluates the prelude's `_.rz`.
The library caches the result per thread, so repeated `Runtime::new()` /
`parse_and_run` calls on the same thread don't repeat that work. If you embed
rizz, prefer reusing a single [`Runtime`](../embedding/driving.md) for a session
over spinning up a fresh one per evaluation — it keeps both the prelude and your
accumulated bindings warm.

## Rules of thumb

- Iterate with `reduce` / `for` / `loop`, recurse for genuinely tree-shaped
  problems within the depth cap.
- Let persistence do the copying; don't hand-roll mutation for speed.
- Reuse a `Runtime` across evaluations in a session.
- Profile before optimizing — most embedded scripts are nowhere near these
  limits.

---

_See also:_ [Refs and Mutability](../language/refs.md) ·
[Control Flow](../language/control-flow.md) ·
[Error Handling](../embedding/errors.md) · _SPEC.md_ §13
