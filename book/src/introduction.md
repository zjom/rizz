# Introduction

**rizz** is a small, dynamically typed Lisp. It is a tree-walking interpreter
shipped as a Rust library, designed to be embedded inside a host application —
a game, a config system, a build tool, a REPL — wherever you want a tiny
scripting language you can drive from Rust and extend with your own functions.

A rizz program is a sequence of S-expressions evaluated against an environment.
There is very little surface area: a handful of special forms, a value-oriented
data model with one explicit escape hatch for mutation, and a standard library
that is partly written in Rust and partly written in rizz itself.

```clojure
;; bind, then use
(let x 10)
(+ x 5)              ;; => 15
```

## What rizz looks like

```clojure
;; recursion
(fn fact (n)
  (if (< n 1) 1
    (* n (fact (- n 1)))))
(fact 5)             ;; => 120

;; a data pipeline
(reduce + 0 (filter (fn p (x) (> x 2)) (range 0 6)))   ;; => 12

;; arrays, maps, and first-class functions
(fmap (fn _ (x) (* x x)) [1 2 3])                       ;; => [1 4 9]
(get { "name" : "ada" } "name")                         ;; => "ada"
```

## The shape of the language

Three things make a rizz program tick:

- **Forms** — the data the program is made of. Source text parses into
  S-expressions, and those S-expressions _are_ the program. Code and data
  share one representation, which is what makes the macro system possible.
- **An environment** — a set of name bindings. Evaluation always happens
  _against_ an environment.
- **Evaluation** — the rule that transforms a form against an environment into
  a value (and a possibly-extended environment).

Everything else — closures, refs, modules, control flow — is built on top of
those three ideas.

Under the hood, source bytes travel through three stages:

| Stage        | What happens                                                       |
| ------------ | ------------------------------------------------------------------ |
| **Parse**    | Bytes become S-expressions (`Sexp`), each tracking its position.   |
| **Lower**    | Each `Sexp` is rewritten into a runtime `Value` — the tree walked. |
| **Evaluate** | The evaluator walks the `Value`, threading an environment through. |

There is no separate runtime AST: the `Value` type doubles as both the data
your program manipulates and the tree the interpreter walks. `(+ 1 2)` is
literally a linked list of three values.

## A few defining choices

rizz makes some deliberate decisions that shape how you write it. They come up
again and again, so they are worth stating up front:

- **Value-oriented by default.** Bindings, closures, arrays, and maps are
  immutable. "Modifying" a collection returns a new, structurally-shared copy.
  The _only_ way to mutate in place is a [`ref`](language/refs.md).
- **No implicit number coercion.** `int` and `float` never mix silently;
  `(+ 1 2.0)` is a type error. Convert explicitly with `int-of` / `float-of`.
- **Booleans are integers.** There is no boolean type — `1` is true and `0` is
  false, and several other values count as false too (see
  [Values and Types](language/values.md)).
- **Errors come in three flavors.** Structural bugs (unbound name, wrong arity)
  always abort; expected failures are returned as values; and a deliberate
  unwind across many frames uses [exceptions](language/errors.md).

## Who this book is for

This is a **hands-on guide**. It teaches the language from the ground up with
runnable examples, then covers idioms and how to embed rizz in a Rust
application. It does not assume you already know Lisp, but a little familiarity
with parentheses-first syntax will not hurt.

The book is organized in four parts:

1. **The Language** — syntax, the value model, evaluation, and every feature
   you will use day to day.
2. **Best Practices & Idioms** — how experienced rizz code is structured, and
   the traps to avoid.
3. **Embedding in Rust** — driving the interpreter, exchanging values with
   Rust, and exposing your own functions.
4. **Appendix** — quick-reference tables for the standard library and grammar.

## This book vs. the spec

The repository ships a formal specification, [`SPEC.md`][spec], which is the
authoritative source of truth for the language's behavior, down to the
edge cases. This book is the friendly counterpart: it teaches and motivates,
and points you at the relevant spec section when you want the exact rule.
Throughout, "See also" footers reference both sibling chapters and the matching
spec section.

[spec]: https://github.com/zjom/rizz/blob/main/SPEC.md

Ready? Start with [Installation & the CLI](getting-started/installation.md).
