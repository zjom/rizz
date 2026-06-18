# The Evaluation Model

This is the most important chapter in the book. Once the evaluation model
clicks, the rest of the language follows from it — scoping, `do`, modules, even
why macros discard their bindings.

## Evaluation takes an environment in and gives one back

The core rule is:

> Evaluating a form takes an **environment in** and returns a **value** *and a
> (possibly extended) environment out*.

In pseudo-notation: `eval(form, env) → (value, env')`.

An **environment** is a set of name bindings. Most forms hand back the same
environment they were given. But the binding forms — `let`, `let!`, `fn` — hand
back an environment that has one more name in it. That returned environment is
threaded into whatever evaluates next.

This single mechanism explains how a binding becomes visible later:

```clojure
(let x 10)     ;; returns 10, and an env where x = 10
(+ x 5)        ;; evaluated against that env → 15
```

The top-level driver threads the environment from each form into the next, which
is why `x` is visible on the second line. The same threading happens inside
[`do`](special-forms.md) and across the arguments of a single call.

## Self-evaluating forms

Some forms evaluate to themselves, unchanged:

- `int`, `float`, `str`, `unit` (`()`), and already-built functions.

Arrays and maps evaluate each element (and key/value) in the surrounding
environment, but **independently**: a binding made while evaluating one element
does not thread to its siblings and does not leak out of the literal. Each slot
is its own little scope.

```clojure
[(let a 1) a]   ;; error — the second slot can't see `a` from the first
```

## Identifiers

An identifier evaluates by **looking it up** in the environment. An unbound name
is an `UnknownIdent` error:

```clojure
(let name "ada")
name              ;; => "ada"
nope              ;; UnknownIdent — never bound
```

## Lists: calls and special forms

A non-empty list `(head . rest)` is evaluated in one of two ways, decided by the
`head`:

1. If `head` is one of the reserved [special-form](special-forms.md) keywords
   (`let`, `fn`, `if`, `do`, `quote`, …), the whole form is dispatched as that
   special form. **This check happens before environment lookup**, so the
   keywords are lexically reserved (more on that below).
2. Otherwise the form is a **function application**:
   - Evaluate `head` to get a callable.
   - Evaluate each argument in `rest`, left to right, threading the environment
     across them.
   - Call the callable on the resulting values.

```clojure
(+ 1 2)                 ;; application: + is looked up, args evaluated, called
(if (< 1 2) 'a 'b)      ;; special form: only one branch is evaluated
```

If `head` evaluates to something that is not callable, you get a `NotCallable`
error. If it evaluates to a [`ref`](refs.md) whose contents are callable, the
ref is peeled and the call proceeds against the contents.

## Argument evaluation threads the environment

Arguments to a single call are evaluated left to right, and bindings made by an
earlier argument are visible to later arguments of the *same* call:

```clojure
(+ (let x 5) x)    ;; => 10   — the second arg sees x from the first
```

But those bindings are **dropped when the call returns** — they do not escape:

```clojure
(+ (let x 5) x)    ;; => 10
x                  ;; UnknownIdent — x did not survive the call
```

This "threads within, drops on return" behavior is the call boundary, and it is
what keeps function calls from polluting the caller's scope.

## Lexical scoping and capture by snapshot

rizz is lexically scoped. A [closure](functions.md) captures the environment as
it existed **at the moment of definition** — a snapshot, not a live reference.
Rebinding a name afterward does not change what the closure sees:

```clojure
(let n 1)
(fn get-n () n)    ;; captures n = 1
(let n 999)        ;; a new binding; doesn't touch the closure's snapshot
(get-n)            ;; => 1
```

Two consequences worth stating plainly:

- **Calls are scope boundaries.** A function's internal `let`/`fn` bindings
  never leak back to the caller.
- The only way to share *mutable* state across a call boundary is to capture a
  [`ref`](refs.md), because the ref is a cell whose identity is captured, not
  its contents.

The exception to "calls are boundaries" is `do`, which is *not* a boundary —
its bindings leak to the enclosing form on purpose. That is the whole point of
`do`, and [Special Forms](special-forms.md) covers it.

## Reserved identifiers

Because the special-form check happens before lookup, the keywords are reserved
*in head position* — you cannot redefine what `(let ...)` means by binding
`let`:

```clojure
(let let 5)        ;; binds the value 5 to the name `let`
let                ;; => 5   — fine as a value
(let x 1)          ;; still the special form — `let` in head position
```

The full reserved set:

```text
let   let!   fn   defmacro   if   do   eval
quote   quasi   unquote   unquote-splice
open   load   load-quoted   try   exception
```

(`doc` is reserved only inside a binding form's documentation slot; see
[Documentation](documentation.md).) The reader-macro prefixes `'`, `` ` ``, `,`,
`,@` expand to `quote`, `quasi`, `unquote`, `unquote-splice`.

Everything *not* in that list — including `+`, `cond`, `for`, `while` — is an
ordinary binding you can shadow. Notably the control-flow forms like `cond` and
`while` are [macros](macros.md) defined in the prelude, not reserved keywords.

---

*See also:* [Special Forms](special-forms.md) ·
[Bindings and Functions](functions.md) · [Refs and Mutability](refs.md) ·
[Reserved Identifiers](../appendix/reserved.md) · *SPEC.md* §4
