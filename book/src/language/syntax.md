# Syntax

rizz has a tiny surface syntax. There are four kinds of atom, three kinds of
compound form (lists, arrays, maps), and a few reader-macro shortcuts. This
chapter is a tour of all of it.

## Whitespace and comments

Spaces, tabs, carriage returns, and newlines are whitespace. They separate
tokens but are otherwise insignificant — indentation carries no meaning.

A comment starts with `;;` and runs to the end of the line:

```clojure
;; a full-line comment
(+ 1 2)   ;; a trailing comment
```

> **Trap:** a single `;` that is *not* followed by another `;` is a syntax
> error (the parser calls it `StraySemicolon`). Comments are always `;;`.

## Atoms

There are four atomic token kinds.

| Atom       | Looks like                  | Notes                                              |
| ---------- | --------------------------- | -------------------------------------------------- |
| **Int**    | `42`, `-7`, `0`             | 64-bit signed. Overflow at parse time is an error. |
| **Float**  | `3.14`, `-0.5`, `1.`        | 64-bit IEEE-754. `1.` parses as `1.0`.             |
| **String** | `"hi"`, `"line\n"`          | UTF-8, double-quoted, with escapes (below).        |
| **Ident**  | `+`, `foo`, `str-join`, `<` | A name. Very permissive (below).                   |

### Integers and floats

An integer is an optional `-` followed by digits. A float has a decimal point:
`1.5`, `-0.25`, or even `1.` (which means `1.0`). Two dots is an error.

A leading `-` immediately followed by a digit is parsed as a negative number;
otherwise `-` begins an identifier (which is why `-` and `->` are valid names).

```clojure
-7        ;; the integer negative seven
-        ;; the identifier "minus" (the subtraction function)
```

There is no implicit conversion between `int` and `float` anywhere in the
language — see [Values and Types](values.md).

### Strings

Strings are double-quoted and must be valid UTF-8. The recognized escapes are
`\\`, `\"`, `\n`, `\r`, and `\t`. Any other `\x` is a parse error.

```clojure
"tab\tseparated"
"she said \"hi\""
"two\nlines"
```

### Identifiers

An identifier is a run of bytes ending at whitespace or one of the delimiter
characters `(` `)` `[` `]` `{` `}` `;`. That makes the set of legal names very
wide: `+`, `-`, `*`, `/`, `<`, `>=`, `=`, `str->int`, `empty?`, `set!`,
`foo.bar` are all single identifiers.

Identifiers are **interned**: two occurrences of the same name share one
underlying string, which makes name comparison cheap.

A few names are reserved as [special forms](special-forms.md) when they appear
in head position — `let`, `fn`, `if`, and friends — but they are still ordinary
identifiers everywhere else. See [Reserved Identifiers](../appendix/reserved.md).

## Compound forms

### Lists — `( ... )`

A list is zero or more forms between parentheses. In a program, a non-empty list
is usually a **call** or a **special form** (covered in
[Evaluation](evaluation.md)); as data (under a [quote](#quoting)) it is just a
linked list.

```clojure
(+ 1 2 3)
(fn square (x) (* x x))
```

The empty list `()` is special: it parses to **nil**, also called **Unit**. Nil
is rizz's "nothing" value and its empty list, all at once.

### Arrays — `[ ... ]`

Square brackets build an array (a persistent vector). Elements are separated by
whitespace — there are no commas:

```clojure
[1 2 3]
["ada" "alan" "grace"]
[(+ 1 1) (* 2 2)]      ;; => [2 4]  — elements are evaluated
```

Arrays are evaluated element by element, each in its own scope (a binding made
inside one element does not leak to the next, or out of the literal).

### Maps — `{ key : value, ... }`

Curly braces build a map (a persistent hash map). Each entry is `key : value`,
and entries are separated by whitespace (a trailing comma between entries is
conventional but the separator is whitespace):

```clojure
{ "name" : "ada"  "born" : 1815 }
{ 1 : "one"  2 : "two" }
```

Keys may be any value, not just strings — numbers, strings, even nested
collections. Like arrays, each key and value is evaluated independently.
Insertion order is **not** preserved.

## Quoting

Four reader-macro prefixes expand into ordinary forms. They are how you write
*data* that looks like *code* without it being evaluated.

| Prefix | Expands to              | Name             |
| ------ | ----------------------- | ---------------- |
| `'X`   | `(quote X)`             | quote            |
| `` `X ``| `(quasi X)`            | quasiquote       |
| `,X`   | `(unquote X)`           | unquote          |
| `,@X`  | `(unquote-splice X)`    | unquote-splicing |

`'x` gives you the *identifier* `x` rather than its value; `'(a b c)` gives you
a three-element list as data. Quasiquote lets you build data with holes punched
in it:

```clojure
'foo                          ;; => the ident foo
'(1 2 3)                      ;; => the list (1 2 3)
`(1 ,(+ 1 1) ,@'(3 4 5))      ;; => (1 2 3 4 5)
```

(Splicing flattens a **cons list**; an array would be inserted as a single
element — see [Special Forms](special-forms.md).)

The semantics are covered in [Special Forms](special-forms.md); the macro use
cases in [Macros and Metaprogramming](macros.md).

## Dotted (improper) lists

Normally a list ends in nil: `(a b c)` is `a`, then `b`, then `c`, then nil. A
standalone `.` between elements makes a **dotted** (improper) list whose tail is
the form after the dot instead of nil:

```clojure
(a b . c)     ;; cons a onto (cons b onto c) — tail is c, not nil
```

The dot is only recognized when surrounded by whitespace, so it never
interferes with floats (`1.5`) or dotted identifiers (`foo.bar`). Exactly one
form may follow the dot.

You will mostly meet dotted lists in **variadic parameter lists** — `(fn f (a b
. rest) ...)` collects extra arguments into `rest`. See
[Bindings and Functions](functions.md).

## Parse errors

Malformed source is rejected *before* any evaluation begins, and every error
points at the line and column of the offending byte. Stray closing brackets,
unterminated lists, a lone `;`, malformed numbers, and invalid string escapes
are all parse errors.

```console
$ echo '(1 2' | cargo run --features cli -- parse
Error: unexpected end of input ...
```

Because parsing happens up front, a syntax error anywhere in a file means no
part of it runs.

---

*See also:* [Values and Types](values.md) ·
[The Evaluation Model](evaluation.md) · [Grammar](../appendix/grammar.md) ·
*SPEC.md* §2
