# rizz — Language Specification

rizz is a small, dynamically typed Lisp. A program is a sequence of
s-expressions evaluated against an environment. This document defines the
surface syntax, the value model, the evaluation rules, and the standard
environment.

---

## Contents

1. [Overview](#1-overview)
2. [Syntax](#2-syntax)
3. [Values](#3-values)
4. [Evaluation](#4-evaluation)
5. [Special forms](#5-special-forms)
6. [Functions](#6-functions)
7. [Refs and mutability](#7-refs-and-mutability)
8. [Modules](#8-modules)
9. [Control flow (prelude macros)](#9-control-flow-prelude-macros)
10. [Standard library](#10-standard-library)
11. [Documentation (`doc` / `show`)](#11-documentation-doc--show)
12. [Errors](#12-errors)
13. [Evaluation model notes](#13-evaluation-model-notes)
14. [Examples](#14-examples)

---

## 1. Overview

A program is one or more **top-level forms** separated by whitespace and/or
line comments. The program's value is the value of the **last** form. Empty or
comment-only input is a parse error.

```
;; bind, then use
(let x 10)
(+ x 5)              ;; program value: 15
```

A program evaluates forms left to right, threading a single environment:

- Each form runs against the environment produced by the previous form.
- A binding introduced by one top-level form is visible to every later
  top-level form.
- The initial environment is the **prelude**.

Three things make a rizz program tick: forms (data), an environment (a stack
of name bindings), and evaluation (which transforms forms against an env).
Everything else — special forms, closures, refs, modules — is built on top.

---

## 2. Syntax

### 2.1 Whitespace

The bytes ``, `\t`, `\r`, `\n` are whitespace. Whitespace separates tokens
but is otherwise insignificant.

### 2.2 Comments

`;;` starts a line comment that runs to the next newline (or EOF). A single
`;` not followed by another `;` is a syntax error (`StraySemicolon`) — use
`;;`.

```
;; this is a comment
(+ 1 2) ;; trailing
```

### 2.3 Atoms

Four atomic token kinds:

| Atom   | Syntax                                                                                  | Notes                                                         |
| ------ | --------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| Int    | `-?[0-9]+`                                                                              | 64-bit signed. Overflow at parse time is an error.            |
| Float  | `-?[0-9]+\.[0-9]*`                                                                      | IEEE-754 64-bit. `1.` parses as `1.0`. Two dots → error.      |
| String | `"..."` with escapes `\\`, `\"`, `\n`, `\r`, `\t`                                       | Any other `\x` is an error. Must be UTF-8.                    |
| Ident  | A run of bytes terminated by ``, `\t`, `\r`, `\n`, `(`, `)`, `[`, `]`, `{`, `}`, or `;` | May include `-`, `+`, `<`, `>`, `=`, `?`, `!`, `*`, `/`, etc. |

A leading `-` followed by a digit dispatches to number parsing; otherwise
`-` begins an identifier. Identifiers are interned: equal names share one
underlying string.

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

### 2.5 Dotted (improper) lists

A standalone `.` between two list elements introduces a **dotted list**:
`(a b . c)` parses to `Cons(a, Cons(b, c))` rather than terminating in
`Unit`. The dot is recognized only when surrounded by whitespace (or followed
by `)`), so it does not interfere with floats (`1.5`) or identifiers that
contain `.` (`foo.bar`). Exactly one form may follow the dot; it becomes the
final tail. The primary use is variadic `fn` parameter lists (§6.1).

### 2.6 Parse errors

Malformed source aborts before evaluation begins. Stray closing delimiters,
unterminated lists, single `;` not followed by another, malformed number
literals, and invalid string escapes are all reported with the line and
column of the offending token.

---

## 3. Values

The value universe:

| Kind       | Description                                                  |
| ---------- | ------------------------------------------------------------ |
| `Str`      | UTF-8 string.                                                |
| `Int`      | 64-bit signed integer.                                       |
| `Float`    | 64-bit IEEE-754 float.                                       |
| `Ident`    | Interned identifier — only present when produced by `quote`. |
| `Unit`     | `()` — also the empty list / nil.                            |
| `Cons`     | Linked list cell with a head and a tail.                     |
| `Array`    | Persistent vector.                                           |
| `Map`      | Persistent hash map; keys may be any Value.                  |
| `NativeFn` | Builtin function.                                            |
| `Closure`  | User-defined function (lexically scoped).                    |
| `Ref`      | Mutable cell — the only mutable value kind (§7).             |

Lists are `Cons` chains terminated by `Unit`. A bare value (a non-cons) is
treated by iteration helpers as a one-element sequence containing itself.

### 3.1 Truthiness

Used by `if`, `not`, `and`, `or`, and prelude macros that branch on a
condition. The following are **false**:

- `()` (Unit)
- `0` (integer zero) and `0.0` (float zero)
- the empty string `""`
- the empty identifier
- an empty array `[]` or empty map `{}`

Everything else — including non-empty lists, non-zero numbers, all closures
and native functions — is **true**.

A `Ref` is truthy iff its current contents are truthy: `(if (ref 0) ...)`
takes the else branch. See §7.

### 3.2 Equality

`=` is structural equality. Lists, arrays, maps, and atoms compare by
contents. Two functions are equal only if they are the same allocation:
closures compare structurally (name/params/body/env); native fns compare by
pointer identity. Refs compare by pointer identity of the underlying cell —
two distinct `(ref 5)` cells are not equal. All NaN floats compare equal.

### 3.3 Numbers and coercion

There is **no implicit coercion** between `int` and `float`. Arithmetic and
comparison are binary and require both operands to be the same numeric kind
(`int×int` or `float×float`). Mixed types raise a `TypeMismatch`. Conversion
is explicit: `int-of` and `float-of` (§10.1) convert between the two kinds
(and parse numeric strings).

Fault policy differs by kind. **Int ops are checked**: overflow and
division by zero raise an `ArithmeticError`. **Float ops follow IEEE-754**:
`(/ 1.0 0.0)` is `inf` and `(/ 0.0 0.0)` is NaN, silently. NaN propagates
through float arithmetic but is rejected wherever an ordering is required —
`cmp`, `min`, `max`, and `clamp` raise an `ArithmeticError` when they
encounter one.

Numeric ops do transparently see through a `Ref` whose contents are a number:
`(+ (ref 5) 1) => 6`. Refs are similarly transparent to `<`, `>=`, etc. This
is the one place values are read through a ref without an explicit `deref`
(see §7.3 for the full list of auto-deref behaviors).

---

## 4. Evaluation

`eval(form, env) → (value, env')`. Every form returns both a value and a
(possibly extended) environment. The threaded environment is how top-level
sequencing, `do`, and `let` inside expressions all communicate bindings to
later forms.

### 4.1 Self-evaluating forms

`Int`, `Float`, `Str`, `Unit`, `NativeFn`, `Closure` evaluate to themselves.

Arrays and maps evaluate each contained element/key/value in the surrounding
env independently. Bindings introduced inside one element do **not** thread to
its siblings and do **not** leak out of the array/map literal — each slot is
its own scope.

### 4.2 Identifiers

An `Ident` is looked up in the env. Unbound → `UnknownIdent`.

### 4.3 Lists (calls and special forms)

A `Cons` is interpreted as `(head . tail)`. If `head` is one of the keyword
identifiers in §4.5, the form is a **special form**. Otherwise it is a
**function application**:

1. Evaluate `head` to obtain a callable. If the result is a `Ref` (or chain
   of refs) whose innermost contents are dispatchable, the refs are peeled
   and dispatch proceeds against the contents.
2. For a `NativeFn`: dispatch to its `call` (which handles arg evaluation).
3. For a `Closure`: evaluate every argument in `tail` left to right,
   threading the env; then call the closure on the resulting values.
4. Any other head value → `NotCallable`.

A call **does not leak** the callee's bindings into the caller: the caller's
env is restored after the call returns.

Argument evaluation order matters: bindings created by earlier arguments
(e.g. `(+ (let x 5) x)`) are visible to later arguments of the same call,
but are dropped once the call returns.

### 4.4 Lexical scoping

Closures capture by snapshot, not by reference: rebinding a name in the outer
env after a closure is defined does not affect what the closure sees. Calls
are env boundaries: a callee's `let`/`fn` bindings never escape back to the
caller. The only way to share mutable state across a call boundary is a
captured `Ref` (§7).

Inside `do`, bindings _do_ leak to enclosing forms — see §5.4.

### 4.5 Reserved identifiers

These names are dispatched as special forms when in **head position** of a
list. They are reserved purely lexically; they are visible to env lookup only
if the runtime's head-dispatch check falls through, which it never does for
keywords. The complete set:

```
let   let!   fn   defmacro   if   do   eval
quote   quasi   unquote   unquote-splice
open   load  load-quoted
```

`doc` is also reserved but only as the head of a `(doc ...)` slot inside a
binding form (§11); outside that context it is a normal identifier.

The reader-macro prefixes `'`, `` ` ``, `,`, `,@` expand to `(quote ...)`,
`(quasi ...)`, `(unquote ...)`, `(unquote-splice ...)` respectively.

---

## 5. Special forms

Each entry below documents one head keyword. Errors marked "arity ≠ N" all
raise `ArityMismatch`.

### 5.1 `let` — bind a value

```
(let NAME VALUE)
(let NAME (doc STR+) VALUE)
```

Evaluates `VALUE`, binds it to `NAME` in the surrounding env, and returns the
bound value. The optional `(doc ...)` slot is described in §11.

Errors: arity ≠ 2 (or 3 when the middle form is `(doc ...)`); `NAME` not an
ident; a malformed `doc` form.

### 5.2 `let!` — bind a ref

```
(let! NAME VALUE)
(let! NAME (doc STR+) VALUE)
```

Sugar for `(let NAME (ref VALUE))`: evaluates `VALUE`, wraps it in a fresh
ref, binds the ref to `NAME`, and returns the ref. Use it whenever you want
a name that will be mutated with `set!`, `push!`, etc. (§7).

```
(let! c 0)
(set! c (+ (deref c) 1))
(deref c)              ;; => 1
```

### 5.3 `fn` — define a function

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
surrounding env _and_ bound under its own name inside the body — that
self-binding is what enables recursion. `PARAMS` is a list of identifiers
(use `()` for zero parameters).

The 3-element shape is disambiguated by whether the middle item is a
`(doc ...)` form: `(fn xs (doc "hi") body)` is anonymous-with-doc, while
`(fn xs (a b) body)` is named (`xs` is the name, `(a b)` the params). An
anonymous closure is not introduced into the surrounding env and cannot
self-reference by name — use a named `fn` for recursion.

A dotted-tail param list `(a b . rest)` makes the function **variadic**: `a`
and `b` are required positional parameters, and any further arguments at the
call site are bundled into a cons list and bound to `rest`. With exactly the
positional count the rest binding is `()`. A bare identifier in the params
position is shorthand for `(. ident)` — zero positional params, all
arguments go to the rest list. Calling a variadic function with fewer than
the positional count is an `ArityMismatch`.

The body is a single form; for multi-step bodies wrap with `do`.

Errors: arity outside `2..=4`; param (positional or rest) not an ident; a
3-element form whose first slot is not an ident when the middle slot is not
a `(doc ...)` form.

### 5.4 `defmacro` — define a macro

```
(defmacro NAME PARAMS BODY)
(defmacro NAME PARAMS (doc STR+) BODY)
```

Defines a user macro and binds it to `NAME`. The shape mirrors `fn`, except
the body receives its arguments **unevaluated**: the macro is invoked at call
sites with the raw forms, and its result is then evaluated in the caller's
env. Parameter lists support the same fixed/dotted/bare-ident variants as
`fn` (so `(defmacro foo xs ...)` collects all argument forms into `xs`).

Only the expansion's _value_ escapes the call site: any bindings introduced
while evaluating the expansion are discarded, so a macro expanding to
`(let x 1)` does **not** bind `x` in the caller's scope. Macros that need to
carry state across forms should expand to `ref` mutations instead.

Macros are the basis of the prelude's control-flow forms (§9). They cannot be
invoked via `apply` — they have no value-level application.

### 5.5 `if` — conditional

```
(if COND THEN ELSE)
```

Evaluates `COND`. If truthy, evaluates `THEN`; otherwise evaluates `ELSE`.
The untaken branch is never evaluated.

Errors: arity ≠ 3.

### 5.6 `do` — sequencing

```
(do FORM*)
```

Evaluates each form in order, threading the env between them, and returns
the last value (or `()` if empty). `do` is **not** a scope boundary: a later
form within the same `do` sees `let`/`fn` bindings introduced by earlier
forms, and those bindings also leak out to the surrounding env. `do` is
pure sequencing — semantically equivalent to splicing its forms into the
enclosing position.

Top-level forms are not wrapped in `do`; the program driver threads
bindings between them explicitly, which is how top-level `let`/`fn` become
visible to later top-level forms. Wrapping a sequence in `do` produces the
same binding-visibility behavior in an expression position.

### 5.7 `quote` — literal data

```
(quote X)            ;; or 'X
```

Returns `X` unevaluated. Identifiers appear as `Ident` values; lists appear
as `Cons` chains.

Errors: arity ≠ 1.

### 5.8 `quasi`, `unquote`, `unquote-splice` — quasiquotation

```
(quasi DATUM)                 ;; or `DATUM
(unquote X)                   ;; or ,X
(unquote-splice X)            ;; or ,@X
```

`quasi` returns `DATUM` as a literal, except:

- An `(unquote X)` subform is replaced by the evaluation of `X`.
- An `(unquote-splice X)` **element** of a list has `X` evaluated and its
  resulting sequence spliced into the surrounding list.

Splicing outside of a surrounding list (e.g. `` `,@xs ``) is a
`TypeMismatch`. `quasi` recurses into nested lists.

This implementation does not support nested quasiquote depth tracking;
unquotes always splice into the nearest enclosing list.

Errors: arity ≠ 1 for `quasi`, `unquote`, and `unquote-splice`.

### 5.9 `eval` — evaluate data as code

```
(eval FORM)
```

Evaluates `FORM` once to get a datum, then evaluates that datum as code in
the current env. Bindings introduced by the evaluated datum extend the
caller's scope, just as with `do`.

```
(let three '(+ 1 2))
(eval three)         ;; => 3
```

### 5.10 `open` / `load` / `load-quoted` — modules

`open` loads a sibling source file and merges its bindings into the caller's
env (optionally under a prefix); `load` returns those bindings as a map; and
`load-quoted` returns the file's forms as unevaluated data. They are meaty
enough to get their own section — see §8.

---

## 6. Functions

### 6.1 Closures

`fn` creates a closure with:

- `name` — the function's own identifier (bound inside the body so it can
  recurse). Anonymous closures have no name and cannot self-reference.
- `params` — a list of positional identifier names.
- `rest` — an optional identifier for the rest parameter (variadic closures
  only; see §5.3).
- `body` — a single form.
- `env` — a snapshot of the lexical env at the point of definition.

Calling a closure binds each positional parameter to its argument in the
captured env (plus the self-binding for recursion), then evaluates the body.
For fixed-arity closures the argument count must match exactly. For variadic
closures the call must supply at least `params.len()` arguments; the
remainder are gathered into a cons list bound to the rest name.

### 6.2 Native functions

Native functions are builtins bound in the initial env. They behave as
ordinary callables: they have a fixed arity, evaluate their arguments, and
return a value. The standard library (§10) lists them all.

### 6.3 Call semantics summary

- Argument evaluation: left to right, env threaded across arguments of a
  single call.
- The call boundary restores the caller's env after the callee returns
  (§4.3).
- A head value that is a ref-of-callable is auto-peeled (§7.3).

---

## 7. Refs and mutability

rizz is mostly value-oriented: `let`/`fn` produce a new env, collections are
persistent, calls don't leak bindings. The exception is the **ref** — a heap
cell whose contents can be replaced in place. Refs are the only path to
mutation; everything else stays immutable.

### 7.1 The core operations

| Name    | Arity | Description                                                                        |
| ------- | ----- | ---------------------------------------------------------------------------------- |
| `ref`   | 1     | `(ref v)`: allocates a new ref initialized to `v`.                                 |
| `deref` | 1     | `(deref r)`: returns the current contents of the cell. Errors on non-ref.          |
| `set!`  | 2     | `(set! r v)`: stores `v` in the cell and returns the new value. Errors on non-ref. |
| `let!`  | 2     | Special form. `(let! NAME v)` ≡ `(let NAME (ref v))`. See §5.2.                    |

Two bindings of the same ref share the cell, so a write through one is
visible through every other binding pointing at it. Closures that capture a
ref capture the cell — not a snapshot of its contents — so mutations made
after the closure was defined are visible inside the body.

`set!` stores its argument verbatim. If `v` is itself a ref, the cell now
aliases it — there is no implicit deref on the way in. Likewise
`(ref (ref x))` is a two-layer ref; both layers must be `deref`d to reach
`x`.

### 7.2 In-place collection ops

Each `!`-suffixed op takes a ref whose cell holds a specific collection kind,
mutates it, and returns the post-mutation value that the cell now holds.
They error if the first argument is not a ref, or if its cell does not hold
the expected inner type. They do not work on bare collections — for
non-mutating updates use the unsuffixed forms (`push`, `pop`, `put`, `del`,
`cons`).

| Name         | Arity | Cell type | Description                        |
| ------------ | ----- | --------- | ---------------------------------- |
| `push!`      | 2     | array     | Appends an element.                |
| `pop!`       | 1     | array     | Removes the last element.          |
| `array-set!` | 3     | array     | Replaces the element at `idx`.     |
| `put!`       | 3     | map       | Inserts `(k → v)`.                 |
| `del!`       | 2     | map       | Removes a key; no-op if absent.    |
| `car!`       | 2     | cons      | Replaces the head; tail preserved. |
| `cdr!`       | 2     | cons      | Replaces the tail; head preserved. |

### 7.3 Where refs are auto-peeled

Most operations treat a ref as opaque — you must `deref` to see through it.
The exceptions, where the runtime transparently looks through a ref:

| Context                     | Behavior                                                                               | Reference |
| --------------------------- | -------------------------------------------------------------------------------------- | --------- |
| Truthiness tests            | A ref is truthy iff its contents are truthy. `(if (ref 0) ...)` takes the else branch. | §3.1      |
| Numeric ops and comparisons | `+`, `-`, `<`, `>=`, etc. read through a ref to a number transparently.                | §3.3      |
| Head position of a call     | A ref-of-callable dispatches as if the ref were the callable directly.                 | §4.3      |
| `show`                      | Reads doc through a ref-of-callable.                                                   | §11       |
| `empty-of`                  | Returns the zero of the _contents_, not a fresh ref (§10.9).                           | §10.9     |

Everything else — equality, `typeof`, collection access, etc. — treats a ref
as itself.

### 7.4 Footguns

- **Equality is by cell identity.** `(= (ref 5) (ref 5))` is `0`. A ref
  equals itself and any binding aliased to it.
- **No auto-collapse on construction.** `ref`, `set!`, `push!`, `put!`,
  `array-set!`, `car!`, `cdr!` all store the value handed to them as-is; nesting refs
  nests storage.
- **`typeof` on a ref returns `ref`**, not the contents' type — use `typeof`
  on a `deref` if you want the inner kind.
- A ref holding a non-callable in head position still errors with
  `NotCallable`; only callable contents auto-peel.

---

## 8. Modules

```
(open PATH)
(open PATH PREFIX)
(load PATH)
(load-quoted PATH)
```

`open` and `load` load the rizz source file at `PATH` and evaluate its
top-level forms in a fresh **module env**; `load-quoted` reads the file but
does **not** evaluate it. `PATH` may be a string or a bare identifier (a
symbol that spells a valid filename); other types raise a `TypeMismatch`.
The three differ in **what they do with the result**:

- `open` **merges all** of the module's top-level bindings into the caller's
  env — including `_`-prefixed names — and returns the value of the module's
  last form. With an optional `PREFIX` ident, each merged name is rewritten
  to `PREFIX.NAME` (e.g. `(open "math" math)` binds `math.sin`), keeping the
  module namespaced. `PREFIX` is taken literally and is **not** evaluated.
- `load` does **not** merge anything; it returns the module's top-level
  bindings as a **map keyed by ident** (`{ sin : <fn>, ... }`). Use it to
  inspect or destructure a module as a first-class value.
- `load-quoted` returns the file's top-level forms as a **list of data** —
  the parsed S-expressions, unevaluated — for metaprogramming.

Path resolution, the fresh module env, and the preserved anchor are
identical across all three.

### 8.1 Path resolution

- If `PATH` has no extension, `.rz` is appended.
- A relative `PATH` is resolved against the caller's source-file directory
  (the env's anchor — set by the entry-point driver and re-anchored on every
  `open` to the opened file's directory). With no anchor, the process CWD is
  used.
- An absolute `PATH` is used verbatim.

Nested `open` resolves relative to the file doing the opening, not the
top-level caller — each loaded module evaluates with its own directory as
the anchor, so a module can `(open "sibling")` portably.

### 8.2 The module env

- The loaded file evaluates against a fresh copy of the prelude, so `+`,
  `cond`, etc. are visible.
- Top-level definitions made in the **caller** do not propagate into the
  module — `open` always loads against a clean module-level scope, not the
  caller's accumulated bindings.

### 8.3 What `open` leaks back to the caller

- **Every** top-level `let`/`fn` binding introduced by the loaded module
  becomes visible in the caller's env — including `_`-prefixed ones (the `_`
  prefix is a naming convention only; `open` does not filter on it).
- With a `PREFIX` ident, each name is rewritten to `PREFIX.NAME` before
  merging, so the bindings stay namespaced and cannot collide with the
  caller's plain names.
- On a name collision the loaded module's new binding wins (the loaded
  module overwrites).
- The caller's `base_dir` anchor is preserved across the call.

`load` and `load-quoted` leak **nothing** into the caller's env; they hand
back a value instead (a map and a list of forms, respectively).

```
;; mod.rz
(let answer 42)
(let _secret 7)
(fn dbl (x) (* x 2))

;; caller.rz — open merges all bindings into scope
(open "mod")     ;; => 84 (last form of mod.rz, if it had one)
(dbl answer)     ;; => 84
_secret          ;; => 7 — open leaks private bindings too

;; open with a prefix namespaces every binding
(open "mod" m)   ;; binds m.answer, m._secret, m.dbl
(m.dbl m.answer) ;; => 84

;; load returns the bindings as a map, merging nothing
(let mod (load "mod"))   ;; => { answer : 42, _secret : 7, dbl : <fn> }
(get mod 'answer)        ;; => 42

;; load-quoted returns the file's forms as unevaluated data
(load-quoted "mod")
;; => ((let answer 42) (let _secret 7) (fn dbl (x) (* x 2)))
```

Errors: `open` accepts 1–2 args, `load`/`load-quoted` exactly 1 (other
arities raise `ArityMismatch`); `PATH` not a string/ident raises
`TypeMismatch`; I/O failure opening the file; any parse error (all three)
or runtime error (`open`/`load`) from the loaded module.

---

## 9. Control flow (prelude macros)

Defined in the prelude via `defmacro`, so they behave like special forms
but are ordinary (shadowable) bindings rather than reserved identifiers.

### 9.1 `cond` — multi-way conditional

```
(cond (TEST BODY)... )
(cond (TEST BODY)... (else BODY))
```

Walks the clauses left to right. Each clause is a two-element list
`(TEST BODY)`; the first clause whose `TEST` evaluates truthy has its `BODY`
evaluated and returned. A literal `else` in test position always matches.
Later clauses are not evaluated once a match is found. With no clauses, or
when no clause matches and no `else` is present, the result is `()`.

```
(cond ((= 1 2) 10)
      ((= 2 2) 20)
      (else    99))   ;; => 20
(cond)                ;; => ()
```

### 9.2 `match` — dispatch on a value via predicates

```
(match VAL (PRED EXPR)... )
(match VAL (PRED EXPR)... (else EXPR))
```

Evaluates `VAL` exactly once, then walks the clauses left to right. Each
clause is a two-element list `(PRED EXPR)` where `PRED` is a call form
`(FN ARGS...)`; the matched value is implicitly inserted as the **first**
argument, so `PRED` is invoked as `(FN VAL ARGS...)`. The first clause
whose predicate evaluates truthy has its `EXPR` evaluated and returned.
A literal `else` in predicate position always matches. Later clauses are
not evaluated once a match is found. With no clauses, or when no clause
matches and no `else` is present, the result is `()`.

```
(match x ((is 'map)   'mapish)
         ((is 'array) 'arrayish)
         ((is 'float) 'floatish)
         (else        'other))

(match 3 ((< 10) 'small) (else 'big))   ;; => 'small  — expands (< 3 10)
```

### 9.3 `unless` — inverted conditional

```
(unless COND BODY...)
```

Evaluates the body forms in order when `COND` is falsy and returns the
value of the last form. When `COND` is truthy the body is not evaluated and
the result is `()`. Equivalent to `(if COND () (do BODY...))`.

### 9.4 `for` — iterate a sequence

```
(for VAR SEQ BODY...)
```

Evaluates `SEQ`, then for each element binds it to `VAR` and evaluates the
body forms in order. Returns the value of the body on the last iteration,
or `()` if `SEQ` is empty. Accepts anything `reduce` accepts (str / array /
map / list). `for` is expressed in terms of `reduce`, so it does not
provide an accumulator — use a `ref` when one is needed.

```
(let! sum 0)
(for x [1 2 3 4] (set! sum (+ (deref sum) x)))
(deref sum)         ;; => 10
```

### 9.5 `loop` — repeat N times

```
(loop N BODY...)
```

Evaluates `N`, then evaluates the body that many times in sequence. Inside
the body, `__i` is bound to the current iteration index (`0..N`). Returns
the value of the body on the final iteration, or `()` if `N ≤ 0`.

```
(let! c 0)
(loop 7 (set! c (+ (deref c) 1)))
(deref c)           ;; => 7
```

### 9.6 `while` — repeat while truthy

```
(while COND BODY...)
```

Re-evaluates `COND` before each iteration. When `COND` is truthy, evaluates
the body forms in order and loops; when `COND` is falsy, returns the value
of the body from the most recent iteration (or `()` if the body never ran).

```
(let! i 0)
(let! sum 0)
(while (< (deref i) 5)
  (set! sum (+ (deref sum) (deref i)))
  (set! i (+ (deref i) 1)))
(deref sum)         ;; => 10
```

### 9.7 `and`, `or` — short-circuit logic

```
(and A B)
(or A B)
```

Lua-style value semantics: `or` returns `A` if `A` is truthy, otherwise
`B`; `and` returns `B` if `A` is truthy, otherwise `A`. In both, `A` is
evaluated exactly once and `B` is only evaluated when needed. Truthiness is
the standard test from §3.1.

```
(or 5 9)         ;; => 5
(or 0 9)         ;; => 9
(or () 42)       ;; => 42
(and 1 2)        ;; => 2
(and 0 9)        ;; => 0
(and () (/ 1 0)) ;; => () — RHS never evaluated
```

### 9.8 `compose`, `pipe` — function composition

```
(compose F G H ...)
(pipe F G H ...)
```

Both return a unary function built from the given functions. `compose`
applies right-to-left: `(compose F G H)` is equivalent to
`(fn _ (x) (F (G (H x))))`. `pipe` applies left-to-right: `(pipe F G H)` is
equivalent to `(fn _ (x) (H (G (F x))))`. With no arguments either form
returns `id`; with a single argument it returns that function unchanged.

```
(let inc    (fn _ (x) (+ x 1)))
(let double (fn _ (x) (* x 2)))

((compose inc double) 3) ;; => 7   — double then inc: (3*2)+1
((pipe    inc double) 3) ;; => 8   — inc then double: (3+1)*2
```

### 9.9 Function combinators

A handful of small higher-order helpers, defined as prelude functions
(so they are ordinary values — pass them around, partially apply them,
etc.). Like `compose`/`pipe`, they assume the conventional arities noted
below.

```
(const V)         ;; -> a function of any arity that ignores its args and returns V
(flip F)          ;; -> (fn _ (a b) (F b a))            — F binary
(partial F A)     ;; -> (fn _ (b) (F A b))              — F binary; binds first arg
(complement P)    ;; -> (fn _ (x) (! (P x)))            — P unary predicate
(on F G)          ;; -> (fn _ (a b) (F (G a) (G b)))    — combine through a projection
(juxt F G)        ;; -> (fn _ (x) [(F x) (G x)])        — apply both, collect results
(tap F X)         ;; runs (F X) for effect, returns X   — side-effecting identity
```

```
((const 7) 1 2 3)                       ;; => 7
((flip -) 3 10)                         ;; => 7    — (- 10 3)
((partial + 1) 4)                       ;; => 5    — increment
((partial (flip /) 2) 10)              ;; => 5    — halve, binding the second arg
(filter (complement (fn _ (n) (> n 2))) [1 2 3 4]) ;; => [1 2]
((on < len) "a" "bbb")                  ;; => 1    — compare by length
((juxt (partial + 1) (partial * 2)) 5)  ;; => [6 10]
```

---

## 10. Standard library

All builtins are bound in the initial env. `1`/`0` is used for boolean
results.

### 10.1 Arithmetic and comparison

| Name        | Arity | Description                                                                                                                                                     |
| ----------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `+`, `sum`  | 2     | Addition (`int×int` or `float×float`). Overflows error.                                                                                                         |
| `-`, `sub`  | 2     | Subtraction.                                                                                                                                                    |
| `*`, `mul`  | 2     | Multiplication.                                                                                                                                                 |
| `/`, `div`  | 2     | Division. Integer divide-by-zero errors.                                                                                                                        |
| `mod`       | 2     | Least nonnegative remainder of A divided by B. Integer divide-by-zero errors.                                                                                   |
| `cmp`       | 2     | -1, 0, or 1 (`-1.0`, `0.0`, `1.0` for floats). NaN errors.                                                                                                      |
| `>`, `gt`   | 2     | Greater than.                                                                                                                                                   |
| `>=`, `gte` | 2     | Greater or equal.                                                                                                                                               |
| `<`, `lt`   | 2     | Less than.                                                                                                                                                      |
| `<=`, `lte` | 2     | Less or equal.                                                                                                                                                  |
| `min`       | ≥ 1   | Minimum of numbers (all `int` or all `float`). Accepts `n` numbers of the same type, or a single array/list of numbers.                                         |
| `max`       | ≥ 1   | Maximum of numbers (same rules as `min`).                                                                                                                       |
| `clamp`     | 3     | Clamps a number to a `[low, high]` range.                                                                                                                       |
| `int-of`    | 1     | Converts to int: rounds a float to the nearest int (ties to even), parses a str, returns an int unchanged. NaN, out-of-range floats, and unparsable strs error. |
| `float-of`  | 1     | Converts to float: converts an int (rounding when no exact float exists), parses a str, returns a float unchanged. Unparsable strs error.                       |

### 10.2 Equality and boolean

| Name        | Arity | Description                     |
| ----------- | ----- | ------------------------------- |
| `=`, `eq`   | 2     | Structural equality (§3.2).     |
| `!=`, `neq` | 2     | Structural inequality.          |
| `!`, `not`  | 1     | Boolean negation of truthiness. |

Lazy `and` and `or` are defined as prelude macros — see §9.6.

### 10.3 Polymorphic collections

These work uniformly across strings, arrays, maps, and cons lists.

| Name        | Arity | Works on                                | Description                                                                                        |
| ----------- | ----- | --------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `all`       | 2     | str/array/map/list                      | Applies predicate `p` to each element. Returns false if any `p` returns false, else true.          |
| `any`       | 2     | str/array/map/list                      | Returns true if `p` is truthy for any element, else false.                                         |
| `len`       | 1     | str/array/map/list                      | Length (str by char).                                                                              |
| `get`       | 2     | str/array/map/list                      | Index or key lookup; miss → `()`.                                                                  |
| `concat`    | 2     | str+str / arr+arr / map+map / list+list | Join; right map wins on key collisions.                                                            |
| `slice`     | 3     | str/array/list                          | Half-open `[start, end)`, clamped.                                                                 |
| `reverse`   | 1     | str/array/list                          | Reversed copy.                                                                                     |
| `first`     | 1     | str/array/list                          | Head, or `()` if empty.                                                                            |
| `last`      | 1     | str/array/list                          | Tail element, or `()` if empty.                                                                    |
| `rest`      | 1     | str/array/list                          | All but the first.                                                                                 |
| `find`      | 2     | str/array/list                          | Returns the idx of the first element for which `p` is truthy, or `()`.                             |
| `contains?` | 2     | str/array/map/list                      | Substring / element / key test.                                                                    |
| `fmap`      | 2     | str/array/map/list                      | Map a function. For maps, `f` takes `(k v)` and returns `[k' v']`.                                 |
| `fmapi`     | 2     | str/array/map/list                      | Map a function with index. `f` takes `(i x)`. For maps, `f` takes `(i k v)` and returns `[k' v']`. |
| `filter`    | 2     | str/array/map/list                      | Keep where predicate is truthy. For maps, `pred` takes `(k v)`.                                    |
| `reduce`    | 3     | str/array/map/list                      | Left fold from `init`. For maps, `f` takes `(acc k v)`.                                            |
| `zip`       | 2     | str/array/map/list                      | Creates a list of pairs; length is `min(len(a), len(b))`.                                          |

### 10.4 Arrays

| Name         | Arity | Description                                                   |
| ------------ | ----- | ------------------------------------------------------------- |
| `push`       | 2     | Append an element (returns a new array).                      |
| `push!`      | 2     | In-place append on a ref-of-array (§7.2).                     |
| `pop`        | 1     | Remove the last element; empty array stays empty.             |
| `pop!`       | 1     | In-place remove-last on a ref-of-array (§7.2).                |
| `array-set`  | 3     | New array with element at `idx` replaced.                     |
| `array-set!` | 3     | In-place set on a ref-of-array (§7.2).                        |
| `range`      | 2     | Array of ints in `[start, end)`.                              |
| `array-of`   | 1     | Constructs an array with a single value.                      |
| `array-from` | 1     | Constructs an array from `xs`. Traverses if `xs` is iterable. |

### 10.5 Maps

| Name     | Arity | Description                                 |
| -------- | ----- | ------------------------------------------- |
| `put`    | 3     | New map with `(k → v)` inserted.            |
| `put!`   | 3     | In-place insert on a ref-of-map (§7.2).     |
| `del`    | 2     | New map with key removed (no-op if absent). |
| `del!`   | 2     | In-place remove on a ref-of-map (§7.2).     |
| `keys`   | 1     | Array of keys (unspecified order).          |
| `values` | 1     | Array of values (unspecified order).        |

Map keys may be any Value (numbers, strings, nested collections). Insertion
order is not preserved.

### 10.6 Strings

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

### 10.7 Lists (cons)

| Name   | Arity | Description                                                                                                                                     |
| ------ | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `cons` | 2     | `(cons head tail)`: a new cons cell. `tail` is typically a list (a cons chain or `()`) but any value is permitted — improper pairs are allowed. |
| `car`  | 1     | `(car xs)`: the head of a cons cell. `(car ())` is `()`.                                                                                        |
| `car!` | 2     | In-place head replacement on a ref-of-cons (§7.2).                                                                                              |
| `cdr`  | 1     | `(cdr xs)`: the tail of a cons cell. `(cdr ())` is `()`.                                                                                        |
| `cdr!` | 2     | In-place tail replacement on a ref-of-cons (§7.2).                                                                                              |

### 10.8 Refs

Recapped here for completeness; see §7 for semantics.

| Name    | Arity | Description                                |
| ------- | ----- | ------------------------------------------ |
| `ref`   | 1     | Allocate a new ref initialized to a value. |
| `deref` | 1     | Read the cell's current contents.          |
| `set!`  | 2     | Replace the cell's contents; returns new.  |

### 10.9 Reflection

| Name       | Arity | Description                                                                  |
| ---------- | ----- | ---------------------------------------------------------------------------- |
| `typeof`   | 1     | Ident of the type of the value.                                              |
| `show`     | 1     | Doc string attached to a closure/macro/native fn (or `()` if none). See §11. |
| `id`       | 1     | Identity function — returns its argument unchanged.                          |
| `empty-of` | 1     | An "empty" value of the same variant as the argument. See below.             |
| `is`       | 2     | `(is x ty)` returns `x` if `(typeof x)` is `ty`, else `()`. See below.       |

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

`(is x ty)` is a type guard: `ty` is an ident or string naming a type
(the values produced by `typeof`, e.g. `int`, `str`, `array`, `map`,
`cons`, `ref`, `closure`, …). If `(typeof x)` matches `ty`, `x` is
returned unchanged; otherwise the result is `()`. Combined with the
truthiness rules (§3.1), this makes `is` directly usable in `if`/`cond`
and as a filter predicate:

```
(if (is v 'int) (+ v 1) v)
(filter (fn [x] (is x 'str)) xs)
```

Like `typeof`, `is` does not peel refs: `(is (ref 7) 'int)` is `()`,
while `(is (ref 7) 'ref)` returns the ref.

---

## 11. Documentation (`doc` / `show`)

Bindings introduced by `let`, `let!`, `fn`, and `defmacro` may carry an
optional documentation slot via a `(doc ARG+)` form:

```
(let  NAME    (doc ARG+) VALUE)
(let! NAME    (doc ARG+) VALUE)
(fn   NAME    PARAMS (doc ARG+) BODY)
(defmacro NAME PARAMS (doc ARG+) BODY)
```

### 11.1 The `doc` form

The `doc` form takes one or more arguments. Each argument is evaluated in
the surrounding environment and must produce either a string or a collection
(array or cons list) of strings — collections are recursively flattened.
All collected strings are joined with `\n` to form the stored documentation.
This means doc text can be passed as a literal, pulled from a variable, or
built up from a list/array of fragments:

```
(let header "increments a number by 1")
(let lines ["params: `n` int" "returns: int"])
(fn inc (n) (doc header lines) (+ n 1))
(show inc)
;; => "increments a number by 1\nparams: `n` int\nreturns: int"
```

### 11.2 Where the doc lives

The doc is attached to the _value_: closures and macros gain it on their
underlying `Closure`; native fns gain it on the `NativeFn`. For `let` /
`let!`, if the bound value is not a callable (e.g. an int, a string, a
collection), the doc is silently dropped — non-callable values have no doc
slot.

### 11.3 `show`

`show` returns the doc string attached to its argument, or `()` if none is
present. Refs are peeled, so `(show r)` and `(show (deref r))` are
equivalent when `r` holds a callable.

```
(fn inc (n)
  (doc "increments a number by 1"
       "params: `n` int"
       "returns: int")
  (+ n 1))

(show inc)
;; => "increments a number by 1\nparams: `n` int\nreturns: int"

(inc 4)        ;; => 5   — the doc form does not interfere with normal evaluation

(let plain 42)
(show plain)   ;; => ()  — non-callables have no doc

(fn bare (n) (+ n 1))
(show bare)    ;; => ()  — no doc was attached
```

### 11.4 `doc` is context-sensitive

`doc` is reserved as the head of a `(doc ...)` slot inside binding forms; it
is not a special form on its own and does not appear in the §4.5 reserved
identifier list. Outside the doc slot of a binding form, a `(doc ...)` list
evaluates as a normal function application — which fails with
`UnknownIdent("doc")` unless the user has bound `doc` to a callable.

Errors: a `doc` form with zero arguments raises `ArityMismatch`; an
argument that evaluates to anything other than a string or a collection of
strings raises `TypeMismatch`.

---

## 12. Errors

A running rizz program can fail in several ways: an unbound identifier, a
call to a non-callable, the wrong number of arguments, the wrong argument
type, an arithmetic fault (overflow, divide-by-zero, NaN comparison), a
string that fails to parse as a number (`int-of` / `float-of`), a failed
module load, and so on. Errors are reported with the name of the
operation that raised them and enough detail to point at the problem.

**rizz has no `try`/`catch`, no exceptions, and no condition system.** Any
runtime error aborts the program — there is no language construct that
lets a program observe a fault and continue.

When writing programs, prefer to use errors as values.

### 12.1 Errors as values

For recoverable failures, return a value that encodes the outcome.
Quoted idents make convenient tags, and a two-element list pairs the tag
with its payload:

```
(fn parse (s)
  (let n (str->int s))
  (if (= n ()) '('err "not a number")
              `('ok ,n)))

(let result (parse "42"))
(if (= (car result) 'ok)
    (car (cdr result))   ;; => 42
    "handle failure")
```

The convention is one of taste, not enforcement — `'ok` / `'err`, `'some`
/ `'none`, or any other set of quoted idents work equally well. The point
is that the caller inspects the tag with `=` and branches with `if` or
`cond` instead of relying on a recovery mechanism the language does not
provide.

---

## 13. Evaluation model notes

### 13.1 No tail-call optimization

rizz does not perform tail-call optimization: deep recursion in rizz
consumes a proportional amount of call-frame depth, and `while`, being a
recursive macro, is subject to the same limit. Pure-iterative loops over
very long sequences should use `reduce`, `for`, or `loop`, which iterate
without recursion.

Recursion depth is capped (10 000 nested evaluations per thread by
default); exceeding the cap raises a `RecursionLimit` error rather than
crashing the host process. Embedders can tune the cap with
`rizz::runtime::set_recursion_limit`.

### 13.2 Persistent vs mutable data

Arrays and maps are persistent: `push`, `put`, `del`, etc. all return new
structures and share structure with the input. The `!`-suffixed in-place
ops (§7.2) commit the result back into a cell. Persistent sharing means
non-mutating updates are cheap even when the original is later mutated.

---

## 14. Examples

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
(str-join (fmap to-str (range 1 4)) ",")          ;; => "1,2,3"
(reduce + 0 (filter (fn p (x) (> x 2))
                    (range 0 6)))                 ;; => 12
```

A counter via a captured ref:

```
(let c (ref 0))
(fn bump () (set! c (+ (deref c) 1)))
(bump) (bump) (bump)
(deref c)             ;; => 3
```

Sharing state through `let!`:

```
(let! count 0)
(loop 5 (set! count (+ (deref count) 1)))
(deref count)         ;; => 5
```
