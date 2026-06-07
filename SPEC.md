# rizz — Language Specification

rizz is a small, dynamically typed Lisp. A program is a sequence of
s-expressions; the runtime is a tree-walking interpreter over the parsed forms.
This document defines the surface syntax, the value model, the evaluation
rules, and the standard environment.

The reference implementation lives in this repository; `src/lib.rs` is the
entry point, with `parser`, `runtime`, and `prelude` as the three stages.

---

## 1. Program model

A program is one or more **top-level forms** separated by whitespace and/or
line comments. Forms are evaluated left to right, threading a single
environment:

- Each form runs against the environment produced by the previous form.
- A `let` or `fn` introduced by one top-level form is visible to every later
  top-level form.
- The program's value is the value of the **last** form. Empty (or
  comment-only) input is a parse error.

```
(let x 10)        ;; binds x in the program env
(+ x 5)           ;; sees x; program value is 15
```

The initial environment is the `prelude` (when the
program is driven by the default `Runtime::new()` entry point; embedding hosts
can pin a different initial env — see §5.7).

---

## 2. Lexical structure

### 2.1 Whitespace

The bytes ``, `\t`, `\r`, `\n` are whitespace. Whitespace separates tokens
but is otherwise insignificant.

### 2.2 Comments

A `;;` starts a line comment that runs to the next newline (or EOF). A single
`;` not followed by another `;` is a syntax error (`StraySemicolon`) — use
`;;`.

```
;; this is a comment
(+ 1 2) ;; trailing
```

### 2.3 Atoms

Four atomic token kinds:

| Atom   | Syntax                                                                                  | Notes                                                                     |
| ------ | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Int    | `-?[0-9]+`                                                                              | 64-bit signed. Overflow at parse time is an error.                        |
| Float  | `-?[0-9]+\.[0-9]*`                                                                      | IEEE-754 64-bit. `1.` parses as `1.0`. Two dots → error.                  |
| String | `"..."` with escapes `\\`, `\"`, `\n`, `\r`, `\t`                                       | Any other `\x` is an error. Must be UTF-8.                                |
| Ident  | A run of bytes terminated by ``, `\t`, `\r`, `\n`, `(`, `)`, `[`, `]`, `{`, `}`, or `;` | Identifiers may include `-`, `+`, `<`, `>`, `=`, `?`, `!`, `*`, `/`, etc. |

A leading `-` followed by a digit dispatches to number parsing; otherwise `-`
begins an identifier. Identifiers are interned: equal names share one
`Rc<str>`.

### 2.4 Compound forms

| Form             | Syntax                                                                              |
| ---------------- | ----------------------------------------------------------------------------------- |
| List             | `( elem* )` or `( elem+ . tail )` (improper / dotted)                               |
| Array            | `[ elem* ]` (whitespace-separated)                                                  |
| Map              | `{ key : value, ... }` (whitespace-separated entries; `:` separates key from value) |
| Quote            | `'X` ≡ `(quote X)`                                                                  |
| Quasiquote       | `` `X `` ≡ `(quasi X)`                                                              |
| Unquote          | `,X` ≡ `(unquote X)`                                                                |
| Unquote-splicing | `,@X` ≡ `(unquote-splice X)`                                                        |

The empty list `()` parses to **nil** (a.k.a. `Unit`).

A standalone `.` between two list elements introduces a **dotted (improper)
list**: `(a b . c)` parses to `Cons(a, Cons(b, c))` rather than terminating in
`Unit`. The dot is recognized only when surrounded by whitespace (or followed
by `)`), so it does not interfere with floats (`1.5`) or identifiers that
contain `.` (`foo.bar`). Exactly one form may follow the dot; it becomes the
final tail. The primary use is variadic `fn` parameter lists.

Note: a stray top-level `)` is an `UnexpectedCloseParen`; an unterminated list
is reported as a missing `)`.

---

## 3. Values

The runtime value universe:

```
Str       — Rc<str>
Int       — i64
Float     — f64 (NaN-aware via OrderedFloat)
Ident     — Rc<str>            (only present when produced by `quote`)
Unit      — ()                 (also: the empty list, nil)
Cons      — { head, tail }     (linked list cell)
Array     — persistent vector (im::Vector)
Map       — persistent hash map (im::HashMap), keys may be any Value
NativeFn  — builtin function
Closure   — user-defined function (lexically scoped)
Ref       — mutable cell (see §8); the only mutable value kind
```

Lists are `Cons` chains terminated by `Unit`. A bare value (a non-cons) is
treated by iteration helpers as a one-element sequence containing itself.

### 3.1 Truthiness

Used by `if`, `not`, `and`, `or`. The following are **false**:

- `()` (Unit)
- `0` (integer zero) and `0.0` (float zero)
- the empty string `""`
- the empty identifier
- an empty array `[]` or empty map `{}`

Everything else — including non-empty lists, non-zero numbers, all closures
and native functions — is **true**.

A `Ref` is truthy iff its current contents are truthy: `(if (ref 0) ...)` takes
the else branch. See §8.

### 3.2 Equality and hashing

`=` is structural equality. Lists, arrays, maps, and atoms compare by
contents. Two functions are equal only if they are the same allocation
(closures: structural equality of name/params/body/env; native fns: pointer
identity). Refs compare by pointer identity of the underlying cell — two
distinct `(ref 5)` cells are not equal. All NaN floats compare equal.

### 3.3 Numeric coercion

There is none. Arithmetic and comparison are binary and require both operands
to be the same numeric kind (`int*int` or `float*float`). Mixed types raise a
`TypeMismatch`.

Numeric ops do transparently see through a `Ref` whose contents are a number:
`(+ (ref 5) 1) => 6`. Refs are similarly transparent to `<`, `>=`, etc. This
is the one place values are read through a ref without an explicit `deref`.

---

## 4. Evaluation

`eval(form, env) → (value, env')`. Every form returns both a value and a
(possibly extended) environment. The threaded environment is how top-level
sequencing, `do`, and `let` inside expressions all communicate bindings to
later forms.

You can call `eval` in rizz itself to evaluate some quoted data. Note that head must be callable or you'll get a runtime error.

```
(let three = '(+ 1 2))
(eval three) ;; 3
```

### 4.1 Self-evaluating forms

`Int`, `Float`, `Str`, `Unit`, `NativeFn`, `Closure` evaluate to themselves.

Arrays and maps evaluate each contained element/key/value in the surrounding
env independently. Bindings introduced inside one element do **not** thread
to its siblings and do **not** leak out of the array/map literal — each slot
is its own scope.

### 4.2 Identifiers

An `Ident` is looked up in the env. Unbound → `UnknownIdent`.

### 4.3 Lists (calls and special forms)

A `Cons` is interpreted as `(head . tail)`. If `head` is one of the keyword
identifiers below, the form is a `special form`. Otherwise
the form is a **function application**:

1. Evaluate `head` to obtain a callable. If the result is a `Ref` (or chain
   of refs) whose innermost contents are dispatchable, the refs are peeled
   and dispatch proceeds against the contents.
2. For a `NativeFn`: dispatch to its `call` (which handles arg evaluation).
3. For a `Closure`: evaluate every argument in `tail` left to right, threading
   the env; then call the closure on the resulting values.
4. Any other head value → `NotCallable`.

A call **does not leak** the callee's bindings into the caller: the caller's
env is restored after the call returns.

Argument evaluation order matters: bindings created by earlier arguments
(e.g. `(+ (let x 5) x)`) are visible to later arguments of the same call,
but are dropped once the call returns — they do not leak to the caller.

---

## 5. Special forms

Keyword identifiers — recognized only when they appear in **head** position of
a list. They are reserved purely lexically; they may be shadowed only by being
intercepted as a head ident (the runtime checks the head string before falling
through to env lookup).

### 5.1 `let` — define a variable

```
(let NAME VALUE)
(let NAME (doc STR+) VALUE)
```

Evaluates `VALUE`, binds it to `NAME` in the surrounding env, returns the
bound value.

An optional `(doc "...")` form between `NAME` and `VALUE` documents the
binding. When `VALUE` evaluates to a callable (closure, macro, native fn) the
doc is attached to it and surfaced by [`show`](#1110-documentation-show). When
`VALUE` is not callable the doc is silently dropped — non-callable values have
no doc slot.

Errors: arity ≠ 2 (or 3 when the middle form is `(doc ...)`); `NAME` not an
ident; a malformed `doc` form.

### 5.2 `fn` — define a function

```
(fn NAME (PARAMS...) BODY)
(fn NAME (PARAMS... . REST) BODY)   ;; variadic via dotted tail
(fn NAME REST BODY)                 ;; variadic via bare ident — all args bundled
(fn NAME PARAMS (doc STR+) BODY)    ;; optional doc slot
(fn PARAMS BODY)                    ;; anonymous — no env binding
(fn PARAMS (doc STR+) BODY)         ;; anonymous with doc slot
```

Creates a closure capturing the current env (lexical scope) and returns the
closure. When `NAME` is given, the closure is also bound under `NAME` in the
surrounding env and bound under its own name inside the body — that
self-binding is what enables recursion. `PARAMS` is a list of identifiers (use
`()` for zero parameters).

The 3-element shape is disambiguated by whether the middle item is a
`(doc ...)` form: `(fn xs (doc "hi") body)` is anonymous-with-doc, while
`(fn xs (a b) body)` is named (`xs` is the name, `(a b)` the params). An
anonymous closure is not introduced into the surrounding env and cannot
self-reference by name, so it cannot recurse — use a named `fn` for that.

An optional `(doc "..." "..." ...)` form may sit between `PARAMS` and `BODY`.
The strings are joined with `\n` and stored on the closure; the doc is
retrievable via [`show`](#1110-documentation-show).

A dotted-tail param list `(a b . rest)` makes the function **variadic**: `a`
and `b` are required positional parameters, and any further arguments at the
call site are bundled into a cons list and bound to `rest`. With exactly the
positional count the rest binding is `()`. A bare identifier in the params
position is shorthand for `(. ident)` — zero positional params, all arguments
go to the rest list. Calling a variadic function with fewer than the
positional count is an `ArityMismatch`.

The body is a single form; for multi-step bodies wrap with `do`.

Errors: arity outside `2..=4` (or a 3-element form whose first slot is not an
ident when the middle slot is not a `(doc ...)` form); any param (positional or
rest) not an ident.

### 5.3 `if` — conditional

```
(if COND THEN ELSE)
```

Evaluates `COND`. If truthy evaluates `THEN`; otherwise
evaluates `ELSE`. The untaken branch is never evaluated.

Errors: arity ≠ 3.

### 5.4 `do` — sequencing

```
(do FORM*)
```

Evaluates each form in order, threading the env between them, and returns
the last value (or `()` if empty). `do` is **not** a scope boundary: a later
form within the same `do` sees `let`/`fn` bindings introduced by earlier
forms, and those bindings also leak out to the surrounding env. `do` is
pure sequencing — semantically equivalent to splicing its forms into the
enclosing position.

Top-level forms are not wrapped in `do`; the program driver threads bindings
between them explicitly, which is how top-level `let`/`fn` become visible to
later top-level forms. Wrapping a sequence in `do` produces the same
binding-visibility behavior in an expression position.

### 5.5 `quote` — literal

```
(quote X)            ;; or 'X
```

Returns `X` unevaluated. Identifiers appear as `Ident` values; lists appear as
`Cons` chains.

Errors: arity ≠ 1.

### 5.6 `quasi`, `unquote`, `unquote-splice` — quasiquotation

```
(quasi DATUM)                 ;; or `DATUM
(unquote X)                   ;; or ,X
(unquote-splice X)            ;; or ,@X
```

`quasi` returns `DATUM` as a literal, except:

- An `(unquote X)` subform is replaced by the evaluation of `X`.
- An `(unquote-splice X)` **element** of a list has `X` evaluated and its
  resulting sequence spliced into the surrounding list.

Splicing outside of a surrounding list (e.g. `` `,@xs ``) is a `TypeMismatch`.
`quasi` recurses into nested lists.

This implementation does not support nested quasiquote depth tracking;
unquotes always splice into the nearest enclosing list.

Errors: arity ≠ 1 for `quasi`, `unquote`, and `unquote-splice`.

### 5.7 `open` — load and merge a module

```
(open PATH)
```

Loads the rizz source file at `PATH`, evaluates its top-level forms in a fresh
**module env**, and **merges the loaded module's bindings into the caller's
env**. Returns the value of the loaded module's last form. `PATH` may be a
string or a bare identifier (a symbol that spells a valid filename); other
types raise a `TypeMismatch`.

Path resolution:

- If `PATH` has no extension, `.rz` is appended.
- A relative `PATH` is resolved against the caller's source-file directory
  (the env's anchor — set by the entry-point driver and re-anchored on every
  `open` to the opened file's directory). With no anchor, the process CWD is
  used.
- An absolute `PATH` is used verbatim.

Module env:

- The loaded file evaluates against a fresh copy of the runtime's pinned
  **base env** — a snapshot of the env the host installed at runtime
  construction. With the default driver (`Runtime::new`) this is the
  `prelude`, so `+`, `cond`, etc. are visible. Hosts
  that embed rizz can pin a different base env to inject custom builtins into
  every loaded module.
- Top-level definitions made in the **caller** do not propagate into the
  module — `open` always loads against a clean module-level scope, not the
  caller's accumulated bindings.
- If no base env is pinned (e.g. a host evaluating against a hand-built `Env`
  without going through `Runtime`), the module env is empty: the loaded file
  sees no prelude.

Binding leakage:

- Top-level `let`/`fn` bindings introduced by the loaded module become visible
  in the caller's env, **except** those whose names start with `_` — a `_`
  prefix is the convention for module-private items and is filtered on merge.
- On a name collision the caller's existing binding wins (the loaded module
  does not overwrite). The caller's `base_dir` anchor is also preserved.
- `open` is a special form rather than a native function precisely because
  function calls cannot leak bindings into their caller (§4.3); module
  loading is the one operation that must.

Nested `open` resolves relative to the file doing the opening, not the
top-level caller — each loaded module evaluates with its own directory as the
anchor (and with the same pinned base env), so a module can `(open "sibling")`
portably.

```
;; mod.rz
(let answer 42)
(let _secret 7)
(fn dbl (x) (* x 2))

;; caller.rz
(open "mod")     ;; => 84 (last form of mod.rz... if it had one)
(dbl answer)     ;; => 84 — `answer` and `dbl` leaked; `_secret` did not
```

Errors: arity ≠ 1; `PATH` not a string/ident; I/O failure opening the file
(surfaced as `IOError`); any parse or runtime error from the loaded module.

---

## 6. Functions

### 6.1 Closures

`fn` creates a closure with:

- `name`: the function's own identifier (bound inside the body so it can
  recurse).
- `params`: a list of positional identifier names.
- `rest`: an optional identifier for the rest parameter (variadic closures
  only; see §5.2).
- `body`: a single form.
- `env`: a snapshot of the lexical env at the point of definition.

Calling a closure binds each positional parameter to its argument in the
captured env (plus the self-binding for recursion), then evaluates the body.
For fixed-arity closures the argument count must match exactly. For variadic
closures the call must supply at least `params.len()` arguments; the remainder
are gathered into a cons list bound to the rest name.

### 6.2 Native functions

There are three flavors:

- **Pure**: receives evaluated args, returns a value.
- **Impure**: receives evaluated args and the env, may return an updated env.
- **Macro**: receives **unevaluated** args and the env, may return an updated
  env. Macros cannot be invoked via `apply` (no value-level application).

All native fns have a fixed arity (`nargs`), enforced before the body runs.

### 6.3 Lexical scoping & isolation

Closures capture by snapshot, not by reference: rebinding a name in the outer
env after definition does not affect what the closure sees. Calls are env
boundaries: a callee's `let`/`fn` bindings never escape back to the caller.

---

## 7. Collections

Arrays and maps are evaluated structurally: each element/value is evaluated
in order, threading the env. Empty `[]` and `{}` literals are valid and
produce an empty array and empty map respectively.

Map keys may be any Value (numbers, strings, nested collections, etc.).
Insertion order is not preserved.

---

## 8. Mutability

rizz is mostly value-oriented: `let`/`fn` produce a new env, collections are
persistent, calls don't leak bindings. The exception is the **ref** — a
heap cell whose contents can be replaced in place. Refs are the only path
to mutation; everything else stays immutable.

### 8.1 Refs

A `Ref` is a value that holds a single `Value` in a mutable cell. Two
bindings of the same ref share the cell, so a write through one is visible
through every other binding pointing at it. Closures that capture a ref
capture the cell — not a snapshot of its contents — so mutations made after
the closure was defined are visible inside the body.

| Name    | Arity | Description                                                                        |
| ------- | ----- | ---------------------------------------------------------------------------------- |
| `ref`   | 1     | `(ref v)`: allocates a new ref initialized to `v`.                                 |
| `deref` | 1     | `(deref r)`: returns the current contents of the cell. Errors on non-ref.          |
| `set!`  | 2     | `(set! r v)`: stores `v` in the cell and returns the new value. Errors on non-ref. |

`set!` stores its argument verbatim. If `v` is itself a ref, the cell now
aliases it — there is no implicit deref on the way in. Likewise `(ref (ref x))`
is a two-layer ref; both layers must be `deref`d to reach `x`.

### 8.2 In-place collection ops

Each `!`-suffixed op takes a ref whose cell holds a specific collection kind,
mutates it, and returns the post-mutation value that the cell now holds. They
error if the first argument is not a ref, or if its cell does not hold the
expected inner type. They do not work on bare collections — for non-mutating
updates use the unsuffixed forms (`push`, `pop`, `put`, `del`, `cons`).

| Name    | Arity | Cell type | Description                        |
| ------- | ----- | --------- | ---------------------------------- |
| `push!` | 2     | array     | Appends an element.                |
| `pop!`  | 1     | array     | Removes the last element.          |
| `put!`  | 3     | map       | Inserts `(k → v)`.                 |
| `del!`  | 2     | map       | Removes a key; no-op if absent.    |
| `car!`  | 2     | cons      | Replaces the head; tail preserved. |
| `cdr!`  | 2     | cons      | Replaces the tail; head preserved. |

### 8.3 Semantics and footguns

- **Equality is by cell identity.** `(= (ref 5) (ref 5))` is `0`. A ref equals
  itself and any binding aliased to it.
- **Truthiness recurses into the cell** (§3.1).
- **Numeric and comparison ops auto-deref** (§3.3).
- **Head-position auto-deref** (§4.3): a call whose head resolves to a
  ref-of-callable dispatches as if the head were the callable directly. A ref
  holding a non-callable still errors with `NotCallable`.
- **No auto-collapse on construction.** `ref`, `set!`, `push!`, `put!`, `car!`,
  `cdr!` all store the value handed to them as-is; nesting refs nests storage.

---

## 9. Errors

Two top-level error families. Both carry enough detail to point at the
problem.

### 9.1 Parse errors (`ParseError`)

Position-tagged (`line`, `col`, `byte`):

- `UnexpectedCloseParen` — stray `)` at top level.
- `StraySemicolon` — single `;` not followed by `;`.
- `ExpectedToken { expected, got }` — wrong delimiter (`)`, `}`, `]`, `:`).
- `UTF8Error` / `FromUTF8Error` — non-UTF-8 byte in source.
- `ParseFloatError`, `ParseIntError` — malformed/overflowing numbers.
- `IOError` — underlying reader failure, including unexpected EOF.

### 9.2 Runtime errors (`RuntimeError`)

- `UnknownIdent(name)` — unbound identifier.
- `NotCallable { value }` — calling a non-callable.
- `ArityMismatch { name, expected, got }` — wrong number of arguments.
- `TypeMismatch { name, expected, got }` — wrong argument type.
- `ArithmeticError { name, reason }` — overflow, divide-by-zero, NaN compare.

---

## 10. Reserved identifiers

These names are dispatched as special forms when in head position:

```
let   let!  fn   if   do   quote   quasi   unquote   unquote-splice    eval
defmacro   open
```

The reader-macro prefixes `'`, `` ` ``, `,`, `,@` expand to `(quote ...)`,
`(quasi ...)`, `(unquote ...)`, `(unquote-splice ...)` respectively.

---

## 11. Built-in environment

All builtins are bound in the initial env. Names and arities below; see
`src/prelude/` for full semantics. `1`/`0` is used for boolean results.

### 11.1 Arithmetic & comparison (`numbers`)

| Name        | Arity | Description                                                                                                                    |
| ----------- | ----- | ------------------------------------------------------------------------------------------------------------------------------ |
| `+`, `sum`  | 2     | Addition (`int×int` or `float×float`). Overflows error.                                                                        |
| `-`, `sub`  | 2     | Subtraction.                                                                                                                   |
| `*`, `mul`  | 2     | Multiplication.                                                                                                                |
| `/`, `div`  | 2     | Division. Integer divide-by-zero errors.                                                                                       |
| `cmp`       | 2     | -1, 0, or 1 (`-1.0`, `0.0`, `1.0` for floats). NaN errors.                                                                     |
| `>`, `gt`   | 2     | Greater than.                                                                                                                  |
| `>=`, `gte` | 2     | Greater or equal.                                                                                                              |
| `<`, `lt`   | 2     | Less than.                                                                                                                     |
| `<=`, `lte` | 2     | Less or equal.                                                                                                                 |
| `min`       | ≥ 1   | Minimum of numbers (all `int` or all `float`). Accepts either `n` numbers of the same type, or a single array/list of numbers. |
| `max`       | ≥ 1   | Maximum of numbers. Accepts either `n` numbers of the same type, or a single array/list of numbers.                            |
| `clamp`     | 3     | Clamps a number to a `[low, high]` range.                                                                                      |

### 11.2 Equality (`eq`)

| Name        | Arity | Description                        |
| ----------- | ----- | ---------------------------------- |
| `=`, `eq`   | 2     | Structural equality.               |
| `!=`, `neq` | 2     | Structural inequality.             |
| `!`, `not`  | 1     | Boolean negation of truthiness.                                        |
| `and`       | 2     | Lua-style: returns `b` if `a` is truthy, else `a`. Lazy in `b`.        |
| `or`        | 2     | Lua-style: returns `a` if `a` is truthy, else `b`. Lazy in `b`.        |

### 11.3 Polymorphic collections (`collections`)

| Name        | Arity | Works on                                | Description                                                                                         |
| ----------- | ----- | --------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `all`       | 2     | str/array/map/list                      | Applies predicate `p` to each element. Returns false if `p` returns false else true.                |
| `any`       | 2     | str/array/map/list                      | Applies predicate `p` to each element. Returns true if `p` returns true else false                  |
| `len`       | 1     | str/array/map/list                      | Length (str by char).                                                                               |
| `get`       | 2     | str/array/map/list                      | Index or key lookup; miss → `()`.                                                                   |
| `concat`    | 2     | str+str / arr+arr / map+map / list+list | Join; right map wins on key collisions.                                                             |
| `slice`     | 3     | str/array/list                          | Half-open `[start, end)`, clamped.                                                                  |
| `reverse`   | 1     | str/array/list                          | Reversed copy.                                                                                      |
| `first`     | 1     | str/array/list                          | Head, or `()` if empty.                                                                             |
| `find`      | 2     | str/array/list                          | Applies predicate `p` to each element. Returns idx of element that `p` returns true for else unit   |
| `last`      | 1     | str/array/list                          | Tail element, or `()` if empty.                                                                     |
| `rest`      | 1     | str/array/list                          | All but the first.                                                                                  |
| `contains?` | 2     | str/array/map/list                      | Substring / element / key test.                                                                     |
| `fmap`      | 2     | str/array/map/list                      | Map a function. For maps, `f` takes `(k v)` and returns `[k' v']`.                                  |
| `fmapi`     | 2     | str/array/map/list                      | Map a function with index. `f` takes `(i, x)`. For maps, `f` takes `(i k v)` and returns `[k' v']`. |
| `filter`    | 2     | str/array/map/list                      | Keep where predicate is truthy. For maps, `pred` takes `(k v)`.                                     |
| `reduce`    | 3     | str/array/map/list                      | Left fold from `init`. For maps, `f` takes `(acc k v)`.                                             |
| `zip`       | 2     | str/array/map/list                      | Creates a list of pairs; has a max len of min(len(a) len(b))                                        |

### 11.4 Arrays (`array`)

| Name         | Arity | Description                                                  |
| ------------ | ----- | ------------------------------------------------------------ |
| `push`       | 2     | Append an element.                                           |
| `push!`      | 2     | In-place append on a ref-of-array (see §8.2).                |
| `pop`        | 1     | Remove the last element; empty array stays empty.            |
| `pop!`       | 1     | In-place remove-last on a ref-of-array (see §8.2).           |
| `range`      | 2     | Array of ints in `[start, end)`.                             |
| `array-of`   | 1     | Constructs an array with a single value.                     |
| `array-from` | 1     | Constructs an array from `xs`. Traverses if `xs` is iterable |

### 11.5 Maps (`map`)

| Name     | Arity | Description                                 |
| -------- | ----- | ------------------------------------------- |
| `put`    | 3     | New map with `(k → v)` inserted.            |
| `put!`   | 3     | In-place insert on a ref-of-map (see §8.2). |
| `del`    | 2     | New map with key removed (no-op if absent). |
| `del!`   | 2     | In-place remove on a ref-of-map (see §8.2). |
| `keys`   | 1     | Array of keys (unspecified order).          |
| `values` | 1     | Array of values (unspecified order).        |

### 11.6 Strings (`str`)

| Name               | Arity | Description                                                              |
| ------------------ | ----- | ------------------------------------------------------------------------ |
| `to-str`, `str-of` | 1     | Stringify any value (top-level strings unquoted, nested strings quoted). |
| `str-upper`        | 1     | Uppercase.                                                               |
| `str-lower`        | 1     | Lowercase.                                                               |
| `str-trim`         | 1     | Strip surrounding whitespace.                                            |
| `str-split`        | 2     | Split into an array; empty separator → per-char.                         |
| `str-join`         | 2     | Join an array/list with a separator (elements via `to-str`).             |
| `str-replace`      | 3     | Replace all occurrences of a substring.                                  |
| `str->int`         | 1     | Parse a decimal integer (`()` on failure).                               |

### 11.7 Lists (`list`)

| Name   | Arity | Description                                                                                                                                     |
| ------ | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `cons` | 2     | `(cons head tail)`: a new cons cell. `tail` is typically a list (a cons chain or `()`) but any value is permitted — improper pairs are allowed. |
| `car`  | 1     | `(car xs)`: the head of a cons cell. `(car ())` is `()`.                                                                                        |
| `car!` | 2     | In-place head replacement on a ref-of-cons (see §8.2).                                                                                          |
| `cdr`  | 1     | `(cdr xs)`: the tail of a cons cell. `(cdr ())` is `()`.                                                                                        |
| `cdr!` | 2     | In-place tail replacement on a ref-of-cons (see §8.2).                                                                                          |

### 11.8 Mutability (`ref_`)

See §8 for full semantics.

| Name    | Arity | Description                                |
| ------- | ----- | ------------------------------------------ |
| `ref`   | 1     | Allocate a new ref initialized to a value. |
| `deref` | 1     | Read the cell's current contents.          |
| `let!`  | 1     | Equivalent to `(let foo (ref v))`          |
| `set!`  | 2     | Replace the cell's contents; returns new.  |

### 11.9 Reflection (`meta`)

| Name       | Arity | Description                                                                     |
| ---------- | ----- | ------------------------------------------------------------------------------- |
| `typeof`   | 1     | Ident of the type of the value.                                                 |
| `show`     | 1     | Doc string attached to a closure/macro/native fn (or `()` if none). See §11.10. |
| `id`       | 1     | Identity of the value. i.e., returns itself                                     |
| `empty-of` | 1     | An "empty" value of the same variant as the argument. See below.                |

`(empty-of v)` returns a value of the same variant as `v` in its "empty"
or zero state:

| Variant of `v`                  | Result                            |
| ------------------------------- | --------------------------------- |
| `int`                           | `0`                               |
| `float`                         | `0.0`                             |
| `str`                           | `""`                              |
| `ident`                         | the empty ident                   |
| `cons`, `unit`                  | `()`                              |
| `array`                         | `[]`                              |
| `map`                           | `{}`                              |
| `closure`, `macro`, `native-fn` | a nullary callable returning `()` |
| `ref`                           | `(empty-of (deref v))` — peeled   |

Refs are peeled, not preserved: `(empty-of (ref 7))` is `0`, not a fresh
ref holding `0`.

### 11.10 Documentation (`show`)

Bindings introduced by `let`, `let!`, `fn`, and `defmacro` may carry an
optional documentation slot via the `(doc ARG+)` form, where each `ARG`
evaluates to a string or a collection of strings:

```
(let NAME (doc ARG+) VALUE)
(let! NAME (doc ARG+) VALUE)
(fn NAME PARAMS (doc ARG+) BODY)
(defmacro NAME PARAMS (doc ARG+) BODY)
```

The `doc` form takes one or more arguments. Each argument is evaluated in
the surrounding environment and must produce either a string or a collection
(array or cons list) of strings — collections are recursively flattened. All
collected strings are joined with `\n` to form the stored documentation.
This means doc text can be passed as a literal, pulled from a variable, or
built up from a list/array of fragments:

```
(let header "increments a number by 1")
(let lines ["params: `n` int" "returns: int"])
(fn inc (n) (doc header lines) (+ n 1))
(show inc)
;; => "increments a number by 1\nparams: `n` int\nreturns: int"
```

The doc lives on the value itself: closures and macros gain it on their
underlying `Closure`; native fns gain it on the `NativeFn`. For `let`/`let!`,
if the bound value is not a callable (e.g. an int, a string, a collection),
the doc is silently dropped — non-callable values have no doc slot.

`show` returns the doc string attached to its argument, or `()` if none is
present. Refs are peeled, so `(show r)` and `(show (deref r))` are equivalent
when `r` holds a callable.

```
(fn inc (n)
  (doc "increments a number by 1"
       "params: `n` int"
       "returns: int")
  (+ n 1))

(show inc)
;; => "increments a number by 1\nparams: `n` int\nreturns: int"

(inc 4) ;; => 5 — the doc form does not interfere with normal evaluation

(let plain 42)
(show plain) ;; => () — non-callables have no doc

(fn bare (n) (+ n 1))
(show bare) ;; => () — no doc was attached
```

Errors: a `doc` form with zero arguments raises `ArityMismatch`; an argument
that evaluates to anything other than a string or a collection of strings
raises `TypeMismatch`.

#### Reserved name

`doc` is reserved as the head of a `(doc ...)` slot inside binding forms; it
is not a special form on its own and does not appear in the §10 reserved
identifier list. Outside the doc slot of a binding form, a `(doc ...)` list
evaluates as a normal function application — which will fail with
`UnknownIdent("doc")` unless the user has bound `doc` to a callable.

### 11.11 Control flow (prelude macros)

Defined in `src/prelude/_.lisp` via `defmacro`, so they behave like special
forms but are ordinary (shadowable) bindings rather than reserved identifiers.

#### `cond` — multi-way conditional

```
(cond (TEST BODY)... )
(cond (TEST BODY)... (else BODY))
```

Walks the clauses left to right. Each clause is a two-element list `(TEST
BODY)`; the first clause whose `TEST` evaluates truthy has its `BODY`
evaluated and returned. A literal `else` in test position always matches.
Later clauses are not evaluated once a match is found. With no clauses, or
when no clause matches and no `else` is present, the result is `()`.

```
(cond ((= 1 2) 10)
      ((= 2 2) 20)
      (else    99))   ;; => 20
(cond)                ;; => ()
```

#### `unless` — inverted conditional

```
(unless COND BODY...)
```

Evaluates the body forms in order when `COND` is falsy and returns the value
of the last form. When `COND` is truthy the body is not evaluated and the
result is `()`. Equivalent to `(if COND () (do BODY...))`.

```
(unless (= 1 2) 'ok)   ;; => ok
(unless (= 1 1) 'ok)   ;; => ()
```

#### `for` — iterate a sequence

```
(for VAR SEQ BODY...)
```

Evaluates `SEQ`, then for each element binds it to `VAR` and evaluates the
body forms in order. Returns the value of the body on the last iteration, or
`()` if `SEQ` is empty. Accepts anything `reduce` accepts (str / array / map
/ list). `for` is expressed in terms of `reduce`, so it does not provide an
accumulator — use a `ref` when one is needed.

```
(let! sum 0)
(for x [1 2 3 4] (set! sum (+ sum x)))
(deref sum)         ;; => 10
```

#### `loop` — repeat N times

```
(loop N BODY...)
```

Evaluates `N`, then evaluates the body that many times in sequence. Returns
the value of the body on the final iteration, or `()` if `N` ≤ 0. The loop
index is not exposed as a binding; use [`for`](#for--iterate-a-sequence)
over `(range 0 n)` if the index is needed.

```
(let! c 0)
(loop 7 (set! c (+ c 1)))
(deref c)           ;; => 7
```

#### `while` — repeat while truthy

```
(while COND BODY...)
```

Re-evaluates `COND` before each iteration. When `COND` is truthy, evaluates
the body forms in order and loops; when `COND` is falsy, returns the value
of the body from the most recent iteration (or `()` if the body never ran).
Implemented via a recursive helper, so a `while` that runs N times consumes
N host stack frames — see §13.

```
(let! i 0)
(let! sum 0)
(while (< i 5)
  (set! sum (+ sum i))
  (set! i (+ i 1)))
(deref sum)         ;; => 10
```

#### `and`, `or` — short-circuit logic

```
(and A B)
(or A B)
```

Lua-style value semantics: `or` returns `A` if `A` is truthy, otherwise `B`;
`and` returns `B` if `A` is truthy, otherwise `A`. In both, `A` is evaluated
exactly once and `B` is only evaluated when needed (lazy). Truthiness is the
standard test from §3.1.

```
(or 5 9)        ;; => 5
(or 0 9)        ;; => 9
(or () 42)      ;; => 42
(and 1 2)       ;; => 2
(and 0 9)       ;; => 0
(and () (/ 1 0)) ;; => () — RHS never evaluated
```

#### `compose`, `pipe` — function composition

```
(compose F G H ...)
(pipe F G H ...)
```

Both return a unary function built from the given functions. `compose` applies
right-to-left: `(compose F G H)` is equivalent to `(fn _ (x) (F (G (H x))))`.
`pipe` applies left-to-right: `(pipe F G H)` is equivalent to
`(fn _ (x) (H (G (F x))))`. With no arguments either form returns `id`; with
a single argument it returns that function unchanged.

```
(let inc    (fn _ (x) (+ x 1)))
(let double (fn _ (x) (* x 2)))

((compose inc double) 3) ;; => 7   — double then inc: (3*2)+1
((pipe    inc double) 3) ;; => 8   — inc then double: (3+1)*2
```

---

## 12. Examples

Variadic function via dotted rest:

```
(fn log (level . args)
    (str-join (fmap to-str (cons level args)) " "))
(log "info" "x =" 42)   ;; => "info x = 42"
```

Fully variadic via bare-ident params:

```
(fn sum xs (reduce + 0 xs))
(sum 1 2 3 4)           ;; => 10
```

Recursive factorial:

```
(fn fact (n)
  (if (< n 1) 1
    (* n (fact (- n 1)))))
(fact 5)            ;; => 120
```

A multi-step function body:

```
((fn run (x)
   (do (let y (* x 2))
       (let z (+ y 1))
       (+ y z)))
 3)                  ;; => 13
```

Quasiquote with splicing:

```
`(1 ,(+ 1 1) ,@(range 3 6))   ;; => (1 2 3 4 5)
```

A small pipeline:

```
(str-join (fmap to-str (range 1 4)) ",")   ;; => "1,2,3"
(reduce + 0 (filter (fn p (x) (> x 2))
                     (range 0 6)))         ;; => 12
```

A counter via a captured ref:

```
(let c (ref 0))
(fn bump () (set! c (+ (deref c) 1)))
(bump) (bump) (bump)
(deref c)             ;; => 3
```

---

## 13. Non-goals (current implementation)

- No tail-call optimization; deep recursion can exhaust the host stack.
- No I/O outside the prelude and `open`
- The module system is `open`-only: there is no namespacing
  or selective import — all non-`_` bindings are merged flat into the caller.
- No exception form.
- No nested quasiquote depth tracking.
