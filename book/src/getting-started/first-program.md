# Your First Program

Let's write something slightly larger than `(+ 1 2)` and use it to introduce the
ideas you will lean on constantly: top-level sequencing, `let`, and `fn`.

## A program is a sequence of forms

A rizz program is one or more **top-level forms** separated by whitespace and/or
comments. The program evaluates them left to right, and its value is the value
of the **last** form.

```clojure
(let x 10)
(+ x 5)              ;; program value: 15
```

Two top-level forms here: a `let` that binds `x`, and an expression that uses
it. The binding introduced by the first form is visible to the second. This is
not a special property of the top level — it is the single most important idea
in the language, and the [Evaluation Model](../language/evaluation.md) chapter
explains exactly how it works. For now: **a name bound by one form is visible to
every later form.**

Empty or comment-only input is a parse error — a program must contain at least
one form.

## Comments

A line comment starts with `;;` and runs to the end of the line:

```clojure
;; this whole line is a comment
(+ 1 2) ;; and so is this trailing note
```

A lone `;` that is not followed by another `;` is a syntax error. Always use
`;;`.

## Binding values with `let`

`let` evaluates an expression and binds the result to a name:

```clojure
(let greeting "hello")
(let count 3)
greeting             ;; => "hello"
```

`let` returns the value it bound, so `(let count 3)` evaluates to `3`.

## Defining functions with `fn`

`fn` creates a function. Give it a name, a parameter list, and a body:

```clojure
(fn square (x) (* x x))
(square 6)           ;; => 36
```

A body can be several expressions: they are evaluated in order, and the value
of the last one is the result.

```clojure
(fn describe (n)
  (let doubled (* n 2))
  (let plus-one (+ doubled 1))
  plus-one)
(describe 5)         ;; => 11
```

Functions can call themselves by name, which is how you recurse:

```clojure
(fn fact (n)
  (if (< n 1) 1
    (* n (fact (- n 1)))))
(fact 5)             ;; => 120
```

## Putting it together

Here is a small script you could drop into `greet.rz` and run with
`cargo run --features cli -- -f greet.rz`:

```clojure
;; greet.rz — build a greeting for a list of names

(fn greet (name)
  (concat "Hello, " (concat name "!")))

(let names ["ada" "alan" "grace"])

;; fmap applies greet to each element, producing a new array
(str-join (fmap greet names) "\n")
```

Running it prints:

```text
Hello, ada!
Hello, alan!
Hello, grace!
```

Every piece of that program is something the next chapters cover in depth:
`concat` and `str-join` and `fmap` are [standard library](../language/stdlib.md)
functions, `["..."]` is an [array literal](../language/syntax.md), and `fmap`
treats the array as a [collection](../language/collections.md). But you can
already read it top to bottom, which is the point.

## Trying things quickly

The fastest feedback loop while reading this book is to pipe snippets straight
into the CLI:

```console
$ echo '(reduce + 0 (range 1 5))' | cargo run --features cli --
10
```

Or start a REPL with `-i` and keep a session of bindings alive as you
experiment. Either way, you now have everything you need to follow along.

---

_See also:_ [Syntax](../language/syntax.md) ·
[Bindings and Functions](../language/functions.md) ·
[Special Forms](../language/special-forms.md)
