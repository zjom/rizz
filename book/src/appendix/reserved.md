# Reserved Identifiers

These names are dispatched as [special forms](../language/special-forms.md) when
they appear in **head position** of a list. The check happens _before_
environment lookup, so they are reserved purely lexically: you can bind one as a
value, but `(name ...)` in head position always dispatches as the special form.

```text
let   let!   fn   defmacro   if   do   eval
quote   quasi   unquote   unquote-splice
open   load   load-quoted   try   exception
```

| Keyword                                | Form                               | Chapter                                          |
| -------------------------------------- | ---------------------------------- | ------------------------------------------------ |
| `let`                                  | Bind a value.                      | [Bindings & Functions](../language/functions.md) |
| `let!`                                 | Bind a fresh ref.                  | [Refs](../language/refs.md)                      |
| `fn`                                   | Define a function.                 | [Bindings & Functions](../language/functions.md) |
| `defmacro`                             | Define a macro.                    | [Macros](../language/macros.md)                  |
| `if`                                   | Conditional.                       | [Special Forms](../language/special-forms.md)    |
| `do`                                   | Sequence forms (leaks bindings).   | [Special Forms](../language/special-forms.md)    |
| `eval`                                 | Evaluate data as code.             | [Special Forms](../language/special-forms.md)    |
| `quote`                                | Literal data (`'X`).               | [Special Forms](../language/special-forms.md)    |
| `quasi` / `unquote` / `unquote-splice` | Quasiquotation (`` ` `` `,` `,@`). | [Special Forms](../language/special-forms.md)    |
| `open` / `load` / `load-quoted`        | Load a module.                     | [Modules](../language/modules.md)                |
| `try`                                  | Catch a raised value.              | [Errors & Exceptions](../language/errors.md)     |
| `exception`                            | Declare an exception constructor.  | [Errors & Exceptions](../language/errors.md)     |

## `doc` is context-sensitive

`doc` is **not** in the list above. It is reserved only as the head of the
documentation slot inside a binding form (`let`, `let!`, `fn`, `defmacro`).
Anywhere else, `(doc ...)` is read as an ordinary function call — which fails
with `UnknownIdent("doc")` unless you've bound `doc` to a callable. See
[Documentation](../language/documentation.md).

## Everything else is shadowable

Names _not_ on this list are ordinary bindings, even when they feel like syntax.
In particular the control-flow forms — `cond`, `match`, `unless`, `for`, `loop`,
`while`, `and`, `or` — and the exception helpers `try-with`, `exn?`, `failwith`
are [prelude](../language/control-flow.md) macros and functions, not reserved
keywords. You _can_ shadow them, though doing so will confuse readers.

The reader-macro prefixes `'`, `` ` ``, `,`, `,@` always expand to `quote`,
`quasi`, `unquote`, `unquote-splice` respectively, regardless of any bindings.

---

_See also:_ [The Evaluation Model](../language/evaluation.md) ·
[Special Forms](../language/special-forms.md) · [Grammar](grammar.md)
