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

The initial environment is the [prelude](#11-prelude-builtins).

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
| List             | `( elem* )`                                                                         |
| Array            | `[ elem* ]` (whitespace-separated)                                                  |
| Map              | `{ key : value, ... }` (whitespace-separated entries; `:` separates key from value) |
| Quote            | `'X` ≡ `(quote X)`                                                                  |
| Quasiquote       | `` `X `` ≡ `(quasi X)`                                                              |
| Unquote          | `,X` ≡ `(unquote X)`                                                                |
| Unquote-splicing | `,@X` ≡ `(unquote-splice X)`                                                        |

The empty list `()` parses to **nil** (a.k.a. `Unit`).

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
```

Lists are `Cons` chains terminated by `Unit`. A bare value (a non-cons) is
treated by iteration helpers as a one-element sequence containing itself.

### 3.1 Truthiness

Used by `if` and `not`. The following are **false**:

- `()` (Unit)
- `0` (integer zero) and `0.0` (float zero)
- the empty string `""`
- the empty identifier
- an empty array `[]` or empty map `{}`

Everything else — including non-empty lists, non-zero numbers, all closures
and native functions — is **true**.

### 3.2 Equality and hashing

`=` is structural equality. Lists, arrays, maps, and atoms compare by
contents. Two functions are equal only if they are the same allocation
(closures: structural equality of name/params/body/env; native fns: pointer
identity). NaN floats compare unequal to themselves only at the arithmetic
layer; `OrderedFloat` makes them equal here so values can key maps.

### 3.3 Numeric coercion

There is none. Arithmetic and comparison are binary and require both operands
to be the same numeric kind (`int*int` or `float*float`). Mixed types raise a
`TypeMismatch`.

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
identifiers below, the form is a [special form](#5-special-forms). Otherwise
the form is a **function application**:

1. Evaluate `head` to obtain a callable.
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
```

Evaluates `VALUE`, binds it to `NAME` in the surrounding env, returns the
bound value.

Errors: arity ≠ 2; `NAME` not an ident.

### 5.2 `fn` — define a function

```
(fn NAME (PARAMS...) BODY)
```

Creates a closure capturing the current env (lexical scope), binds it under
`NAME`, and returns the closure. The closure's own name is bound inside the
body, which is what enables recursion. `PARAMS` is a list of identifiers (use
`()` for zero parameters).

The body is a single form; for multi-step bodies wrap with `do`.

Errors: arity ≠ 3; `NAME` not an ident; any param not an ident.

### 5.3 `if` — conditional

```
(if COND THEN ELSE)
```

Evaluates `COND`. If [truthy](#31-truthiness) evaluates `THEN`; otherwise
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

---

## 6. Functions

### 6.1 Closures

`fn` creates a closure with:

- `name`: the function's own identifier (bound inside the body so it can
  recurse).
- `params`: a list of identifier names.
- `body`: a single form.
- `env`: a snapshot of the lexical env at the point of definition.

Calling a closure binds each parameter to its argument in the captured env
(plus the self-binding for recursion), then evaluates the body. Arity must
match exactly.

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

## 8. Errors

Two top-level error families. Both carry enough detail to point at the
problem.

### 8.1 Parse errors (`ParseError`)

Position-tagged (`line`, `col`, `byte`):

- `UnexpectedCloseParen` — stray `)` at top level.
- `StraySemicolon` — single `;` not followed by `;`.
- `ExpectedToken { expected, got }` — wrong delimiter (`)`, `}`, `]`, `:`).
- `UTF8Error` / `FromUTF8Error` — non-UTF-8 byte in source.
- `ParseFloatError`, `ParseIntError` — malformed/overflowing numbers.
- `IOError` — underlying reader failure, including unexpected EOF.

### 8.2 Runtime errors (`RuntimeError`)

- `UnknownIdent(name)` — unbound identifier.
- `NotCallable { value }` — calling a non-callable.
- `ArityMismatch { name, expected, got }` — wrong number of arguments.
- `TypeMismatch { name, expected, got }` — wrong argument type.
- `ArithmeticError { name, reason }` — overflow, divide-by-zero, NaN compare.

---

## 9. Reserved identifiers

These names are dispatched as special forms when in head position:

```
let   fn   if   do   quote   quasi   unquote   unquote-splice eval
```

The reader-macro prefixes `'`, `` ` ``, `,`, `,@` expand to `(quote ...)`,
`(quasi ...)`, `(unquote ...)`, `(unquote-splice ...)` respectively.

---

## 10. Built-in environment

All builtins are bound in the initial env. Names and arities below; see
`src/prelude/` for full semantics. `1`/`0` is used for boolean results.

### 10.1 Arithmetic & comparison (`numbers`)

| Name  | Arity | Description                                                |
| ----- | ----- | ---------------------------------------------------------- |
| `+`   | 2     | Addition (`int×int` or `float×float`). Overflows error.    |
| `-`   | 2     | Subtraction.                                               |
| `*`   | 2     | Multiplication.                                            |
| `/`   | 2     | Division. Integer divide-by-zero errors.                   |
| `cmp` | 2     | -1, 0, or 1 (`-1.0`, `0.0`, `1.0` for floats). NaN errors. |
| `>`   | 2     | Greater than.                                              |
| `>=`  | 2     | Greater or equal.                                          |
| `<`   | 2     | Less than.                                                 |
| `<=`  | 2     | Less or equal.                                             |

### 10.2 Equality (`eq`)

| Name | Arity | Description                     |
| ---- | ----- | ------------------------------- |
| `=`  | 2     | Structural equality.            |
| `!=` | 2     | Structural inequality.          |
| `!`  | 1     | Boolean negation of truthiness. |

### 10.3 Polymorphic collections (`collections`)

| Name        | Arity | Works on                                | Description                                                        |
| ----------- | ----- | --------------------------------------- | ------------------------------------------------------------------ |
| `len`       | 1     | str/array/map/list                      | Length (str by char).                                              |
| `get`       | 2     | str/array/map/list                      | Index or key lookup; miss → `()`.                                  |
| `concat`    | 2     | str+str / arr+arr / map+map / list+list | Join; right map wins on key collisions.                            |
| `slice`     | 3     | str/array/list                          | Half-open `[start, end)`, clamped.                                 |
| `reverse`   | 1     | str/array/list                          | Reversed copy.                                                     |
| `first`     | 1     | str/array/list                          | Head, or `()` if empty.                                            |
| `last`      | 1     | str/array/list                          | Tail element, or `()` if empty.                                    |
| `rest`      | 1     | str/array/list                          | All but the first.                                                 |
| `contains?` | 2     | str/array/map/list                      | Substring / element / key test.                                    |
| `fmap`      | 2     | str/array/map/list                      | Map a function. For maps, `f` takes `(k v)` and returns `[k' v']`. |
| `filter`    | 2     | str/array/map/list                      | Keep where predicate is truthy. For maps, `pred` takes `(k v)`.    |
| `reduce`    | 3     | str/array/map/list                      | Left fold from `init`. For maps, `f` takes `(acc k v)`.            |

### 10.4 Arrays (`array`)

| Name    | Arity | Description                      |
| ------- | ----- | -------------------------------- |
| `push`  | 2     | Append an element.               |
| `range` | 2     | Array of ints in `[start, end)`. |

### 10.5 Maps (`map`)

| Name     | Arity | Description                                 |
| -------- | ----- | ------------------------------------------- |
| `put`    | 3     | New map with `(k → v)` inserted.            |
| `del`    | 2     | New map with key removed (no-op if absent). |
| `keys`   | 1     | Array of keys (unspecified order).          |
| `values` | 1     | Array of values (unspecified order).        |

### 10.6 Strings (`str`)

| Name          | Arity | Description                                                              |
| ------------- | ----- | ------------------------------------------------------------------------ |
| `to-str`      | 1     | Stringify any value (top-level strings unquoted, nested strings quoted). |
| `str-upper`   | 1     | Uppercase.                                                               |
| `str-lower`   | 1     | Lowercase.                                                               |
| `str-trim`    | 1     | Strip surrounding whitespace.                                            |
| `str-split`   | 2     | Split into an array; empty separator → per-char.                         |
| `str-join`    | 2     | Join an array with a separator (elements via `to-str`).                  |
| `str-replace` | 3     | Replace all occurrences of a substring.                                  |
| `str->int`    | 1     | Parse a decimal integer (`()` on failure).                               |

### 10.7 Lists (`list`)

| Name   | Arity | Description                                                                                                                                     |
| ------ | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `cons` | 2     | `(cons head tail)`: a new cons cell. `tail` is typically a list (a cons chain or `()`) but any value is permitted — improper pairs are allowed. |
| `car`  | 1     | `(car xs)`: the head of a cons cell. `(car ())` is `()`.                                                                                        |
| `cdr`  | 1     | `(cdr xs)`: the tail of a cons cell. `(cdr ())` is `()`.                                                                                        |

---

## 11. Examples

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

---

## 12. Non-goals (current implementation)

- No tail-call optimization; deep recursion can exhaust the host stack.
- No variadic functions; arity is exact.
- No mutation: every binding update produces a new env; collections are
  persistent (structural sharing).
- No module system, no I/O outside the prelude, no exception form.
- No nested quasiquote depth tracking.
