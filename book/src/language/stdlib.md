# The Standard Library

The standard library is partly Rust builtins and partly rizz code (the prelude's
`_.rz`). [Collections](collections.md) covered the collection operations; this
chapter covers the rest: arithmetic, comparison, equality, strings, reflection,
and the function combinators. The [Standard Library
Reference](../appendix/stdlib-reference.md) appendix has the consolidated tables;
here we focus on usage.

Remember that boolean-ish results are `1` (true) and `0` (false).

## Arithmetic and comparison

All arithmetic is binary and requires both operands to be the **same** numeric
kind — see [Values and Types](values.md) on the no-coercion rule.

| Operation        | Notes                                                          |
| ---------------- | -------------------------------------------------------------- |
| `+` `-` `*` `/`  | Also spelled `sum` `sub` `mul` `div`. Int overflow/÷0 error.   |
| `mod`            | Least nonnegative remainder.                                   |
| `cmp`            | `-1`, `0`, or `1` (float versions for floats). NaN errors.     |
| `<` `<=` `>` `>=`| Also `lt` `lte` `gt` `gte`.                                    |
| `min` `max`      | Of N numbers (same kind) or one array/list. NaN errors.       |
| `clamp`          | `(clamp x lo hi)` constrains `x` to `[lo, hi]`.                |
| `int-of`         | Float→int (round, ties to even), parse a string, int→int.     |
| `float-of`       | Int→float, parse a string, float→float.                       |

```clojure
(+ 2 3)            ;; => 5
(/ 7 2)            ;; => 3    — integer division
(/ 7.0 2.0)        ;; => 3.5
(mod 7 3)          ;; => 1
(cmp 2 5)          ;; => -1
(min 4 1 7)        ;; => 1
(min [4 1 7])      ;; => 1    — also accepts a single collection
(clamp 12 0 10)    ;; => 10
(int-of 3.7)       ;; => 4
(int-of "42")      ;; => 42
(float-of 3)       ;; => 3.0
```

## Equality and boolean

| Operation     | Notes                              |
| ------------- | ---------------------------------- |
| `=` / `eq`    | Structural equality.               |
| `!=` / `neq`  | Structural inequality.             |
| `!` / `not`   | Negate truthiness → `1` or `0`.    |

```clojure
(= [1 2] [1 2])    ;; => 1
(!= 1 2)           ;; => 1
(! 0)              ;; => 1
(! "anything")     ;; => 0
```

The short-circuiting `and` / `or` are prelude *macros*, covered in
[Control Flow](control-flow.md).

## Strings

| Operation              | Description                                                  |
| ---------------------- | ------------------------------------------------------------ |
| `to-str` / `str-of`    | Stringify any value (nested strings get quoted).             |
| `str-upper`            | Uppercase.                                                   |
| `str-lower`            | Lowercase.                                                   |
| `str-trim`             | Strip surrounding whitespace.                                |
| `str-split`            | Split into an array; `""` separator → per character.         |
| `str-join`             | Join an array/list with a separator.                         |
| `str-replace`          | Replace all occurrences of a substring.                      |
| `str->int`             | Parse a decimal integer, or `()` on failure.                 |

```clojure
(str-upper "hi")               ;; => "HI"
(str-trim "  hi  ")            ;; => "hi"
(str-split "a,b,c" ",")        ;; => ["a" "b" "c"]
(str-split "abc" "")           ;; => ["a" "b" "c"]
(str-join ["a" "b" "c"] "-")   ;; => "a-b-c"
(str-replace "aXbXc" "X" "/")  ;; => "a/b/c"
(str->int "42")                ;; => 42
(str->int "nope")              ;; => ()
(to-str [1 "two" 3])           ;; => "[1 \"two\" 3]"
```

`to-str` is what you reach for when assembling text out of mixed values:

```clojure
(str-join (fmap to-str (range 1 4)) ",")   ;; => "1,2,3"
```

## Reflection

| Operation         | Description                                                       |
| ----------------- | ---------------------------------------------------------------- |
| `(typeof v)`      | The type as an ident: `int`, `str`, `array`, `ref`, `closure`, … |
| `(is v ty)`       | Returns `v` if its type is `ty`, else `()`.                      |
| `(empty-of v)`    | A zero/empty value of the same kind as `v`.                      |
| `(id v)`          | Identity — returns `v` unchanged.                                |
| `(show v)`        | The doc string attached to a callable, or `()`.                  |

`is` is a type guard. Because it returns the value (truthy) on a match and `()`
(falsy) on a miss, it slots straight into `if`, `cond`, `match`, and `filter`:

```clojure
(typeof [1 2])              ;; => array
(is 5 'int)                ;; => 5
(is 5 'str)                ;; => ()
(if (is v 'int) (+ v 1) v) ;; guard before doing int math
(filter (fn _ (x) (is x 'str)) [1 "a" 2 "b"])   ;; => ["a" "b"]
```

Like `typeof`, `is` does not peel refs: `(is (ref 7) 'int)` is `()`, while
`(is (ref 7) 'ref)` returns the ref.

`empty-of` gives you the identity element of a value's type — `0` for ints, `""`
for strings, `[]` for arrays, `{}` for maps:

```clojure
(empty-of 99)      ;; => 0
(empty-of "hi")    ;; => ""
(empty-of [1 2])   ;; => []
(empty-of (ref 7)) ;; => 0   — refs are peeled to their contents' zero
```

`show` surfaces documentation; see [Documentation](documentation.md).

## Function combinators

The prelude defines a set of higher-order helpers in rizz itself. They are
ordinary values — pass them around, partially apply them, store them in
collections.

| Combinator       | Equivalent to                          |
| ---------------- | -------------------------------------- |
| `(compose F G H)`| `(fn _ (x) (F (G (H x))))` — right to left |
| `(pipe F G H)`   | `(fn _ (x) (H (G (F x))))` — left to right  |
| `(const V)`      | a function of any args that returns `V` |
| `(flip F)`       | `(fn _ (a b) (F b a))`                 |
| `(partial F A)`  | `(fn _ (b) (F A b))`                   |
| `(complement P)` | `(fn _ (x) (! (P x)))`                 |
| `(on F G)`       | `(fn _ (a b) (F (G a) (G b)))`         |
| `(juxt F G)`     | `(fn _ (x) [(F x) (G x)])`             |
| `(tap F X)`      | run `(F X)` for effect, return `X`     |

```clojure
(let inc    (fn _ (x) (+ x 1)))
(let double (fn _ (x) (* x 2)))

((compose inc double) 3)   ;; => 7   — double then inc
((pipe    inc double) 3)   ;; => 8   — inc then double
((const 7) 1 2 3)          ;; => 7
((flip -) 3 10)            ;; => 7   — (- 10 3)
((partial + 1) 4)          ;; => 5   — an increment function
((partial (flip /) 2) 10)  ;; => 5   — halve, binding the divisor
((on < len) "a" "bbb")     ;; => 1   — compare by length
((juxt (partial + 1) (partial * 2)) 5)   ;; => [6 10]
(filter (complement (fn _ (n) (> n 2))) [1 2 3 4])   ;; => [1 2]
```

`compose`/`pipe` accept any number of functions: with none they return `id`,
with one they return it unchanged. These combinators assume the conventional
arities shown above (mostly unary and binary).

`tap` is the odd one out — it runs a function for its side effect and returns its
argument, which makes it perfect for slipping a log into a `pipe` chain without
disturbing the value flowing through.

---

*See also:* [Values and Types](values.md) · [Collections](collections.md) ·
[Control Flow](control-flow.md) · [Documentation](documentation.md) ·
[Standard Library Reference](../appendix/stdlib-reference.md) · *SPEC.md* §10
