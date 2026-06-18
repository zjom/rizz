# Standard Library Reference

A consolidated lookup table for the standard environment. Boolean-ish results are
`1` (true) / `0` (false). For usage and examples, see
[The Standard Library](../language/stdlib.md) and
[Collections](../language/collections.md). For the authoritative rules, see
_SPEC.md_ §10.

Arity notes: a number is the exact/minimum argument count; `≥ n` marks variadic.

## Arithmetic & comparison

| Name         | Arity | Description                                                 |
| ------------ | ----- | ----------------------------------------------------------- |
| `+` / `sum`  | 2     | Addition (`int×int` or `float×float`). Int overflow errors. |
| `-` / `sub`  | 2     | Subtraction.                                                |
| `*` / `mul`  | 2     | Multiplication.                                             |
| `/` / `div`  | 2     | Division. Integer divide-by-zero errors.                    |
| `mod`        | 2     | Least nonnegative remainder.                                |
| `cmp`        | 2     | `-1` / `0` / `1` (float versions for floats). NaN errors.   |
| `>` / `gt`   | 2     | Greater than.                                               |
| `>=` / `gte` | 2     | Greater or equal.                                           |
| `<` / `lt`   | 2     | Less than.                                                  |
| `<=` / `lte` | 2     | Less or equal.                                              |
| `min`        | ≥ 1   | Minimum of numbers, or of a single array/list. NaN errors.  |
| `max`        | ≥ 1   | Maximum (same rules as `min`).                              |
| `clamp`      | 3     | Constrain a number to `[low, high]`.                        |
| `int-of`     | 1     | To int: round a float (ties to even), parse a str, int→int. |
| `float-of`   | 1     | To float: int→float, parse a str, float→float.              |

## Equality & boolean

| Name         | Arity | Description            |
| ------------ | ----- | ---------------------- |
| `=` / `eq`   | 2     | Structural equality.   |
| `!=` / `neq` | 2     | Structural inequality. |
| `!` / `not`  | 1     | Negate truthiness.     |

(`and` / `or` are short-circuiting prelude macros — see
[Control Flow](../language/control-flow.md).)

## Polymorphic collection operations

Work across str / array / map / list unless noted.

| Name        | Arity | Description                                                  |
| ----------- | ----- | ------------------------------------------------------------ |
| `len`       | 1     | Length (string by character).                                |
| `get`       | 2     | Index/key lookup; miss → `()`.                               |
| `contains?` | 2     | Substring / element / key test.                              |
| `concat`    | 2     | Join two of the same kind (right map wins on key clash).     |
| `all`       | 2     | True if predicate holds for every element.                   |
| `any`       | 2     | True if predicate holds for any element.                     |
| `fmap`      | 2     | Map a function (maps: `f` takes `(k v)` → `[k' v']`).        |
| `fmapi`     | 2     | Map with index `(i x)` (maps: `(i k v)` → `[k' v']`).        |
| `filter`    | 2     | Keep where predicate is truthy (maps: `pred` takes `(k v)`). |
| `reduce`    | 3     | Left fold from `init` (maps: `f` takes `(acc k v)`).         |
| `zip`       | 2     | List of pairs; length `min(len a, len b)`.                   |
| `slice`     | 3     | str/array/list. Half-open `[start, end)`, clamped.           |
| `reverse`   | 1     | str/array/list. Reversed copy.                               |
| `first`     | 1     | str/array/list. Head, or `()`.                               |
| `last`      | 1     | str/array/list. Last element, or `()`.                       |
| `rest`      | 1     | str/array/list. All but the first.                           |
| `find`      | 2     | str/array/list. Index of first match, or `()`.               |

## Arrays

| Name         | Arity | Description                                       |
| ------------ | ----- | ------------------------------------------------- |
| `push`       | 2     | New array with an element appended.               |
| `push!`      | 2     | In-place append on a ref-of-array.                |
| `pop`        | 1     | New array without the last element.               |
| `pop!`       | 1     | In-place remove-last on a ref-of-array.           |
| `array-set`  | 3     | New array with element at `idx` replaced.         |
| `array-set!` | 3     | In-place set on a ref-of-array.                   |
| `range`      | 2     | Array of ints in `[start, end)`.                  |
| `array-of`   | 1     | Single-element array.                             |
| `array-from` | 1     | Build an array from `xs` (traverses if iterable). |

## Maps

| Name     | Arity | Description                                 |
| -------- | ----- | ------------------------------------------- |
| `put`    | 3     | New map with `k → v` inserted.              |
| `put!`   | 3     | In-place insert on a ref-of-map.            |
| `del`    | 2     | New map with key removed (no-op if absent). |
| `del!`   | 2     | In-place remove on a ref-of-map.            |
| `keys`   | 1     | Array of keys (unspecified order).          |
| `values` | 1     | Array of values (unspecified order).        |

## Strings

| Name                | Arity | Description                                           |
| ------------------- | ----- | ----------------------------------------------------- |
| `to-str` / `str-of` | 1     | Stringify any value (nested strings quoted).          |
| `str-upper`         | 1     | Uppercase.                                            |
| `str-lower`         | 1     | Lowercase.                                            |
| `str-trim`          | 1     | Strip surrounding whitespace.                         |
| `str-split`         | 2     | Split into an array; empty separator → per character. |
| `str-join`          | 2     | Join an array/list with a separator.                  |
| `str-replace`       | 3     | Replace all occurrences of a substring.               |
| `str->int`          | 1     | Parse a decimal integer (`()` on failure).            |

## Lists (cons)

| Name   | Arity | Description                                 |
| ------ | ----- | ------------------------------------------- |
| `cons` | 2     | A new cons cell `(head . tail)`.            |
| `car`  | 1     | Head of a cons; `(car ())` is `()`.         |
| `car!` | 2     | In-place head replacement on a ref-of-cons. |
| `cdr`  | 1     | Tail of a cons; `(cdr ())` is `()`.         |
| `cdr!` | 2     | In-place tail replacement on a ref-of-cons. |

## Refs

| Name    | Arity | Description                                |
| ------- | ----- | ------------------------------------------ |
| `ref`   | 1     | Allocate a new ref initialized to a value. |
| `deref` | 1     | Read the cell's current contents.          |
| `set!`  | 2     | Replace the cell's contents; returns new.  |

## Reflection

| Name       | Arity | Description                                             |
| ---------- | ----- | ------------------------------------------------------- |
| `typeof`   | 1     | The value's type, as an ident.                          |
| `is`       | 2     | `(is x ty)` → `x` if its type is `ty`, else `()`.       |
| `empty-of` | 1     | An "empty"/zero value of the same kind as the argument. |
| `id`       | 1     | Identity — returns its argument unchanged.              |
| `show`     | 1     | Doc string attached to a callable, or `()`.             |

## Function combinators (prelude)

| Name         | Description                                             |
| ------------ | ------------------------------------------------------- |
| `compose`    | Compose functions right-to-left.                        |
| `pipe`       | Compose functions left-to-right.                        |
| `const`      | A function that ignores args and returns a fixed value. |
| `flip`       | Swap a binary function's two arguments.                 |
| `partial`    | Bind a binary function's first argument.                |
| `complement` | Negate a unary predicate.                               |
| `on`         | Combine two values through a projection.                |
| `juxt`       | Apply two functions, collect results in an array.       |
| `tap`        | Run a function for effect, return the argument.         |

## Control flow & exceptions (prelude macros / forms)

| Name         | Kind         | Description                                        |
| ------------ | ------------ | -------------------------------------------------- |
| `cond`       | macro        | Multi-way conditional.                             |
| `match`      | macro        | Dispatch on a value via predicates.                |
| `unless`     | macro        | Run body when condition is falsy.                  |
| `for`        | macro        | Iterate a sequence (no accumulator; use a ref).    |
| `loop`       | macro        | Repeat N times; `__i` is the index.                |
| `while`      | macro        | Repeat while a condition is truthy (recursive).    |
| `and` / `or` | macro        | Short-circuit, value-returning logic.              |
| `exception`  | special form | Bind a name to an exception constructor.           |
| `raise`      | native fn    | Raise a value to the nearest `try`.                |
| `try`        | special form | Catch a raised value (`catch` / `finally`).        |
| `try-with`   | macro        | Catch and dispatch by constructor.                 |
| `exn?`       | function     | Test whether a value is tagged with a constructor. |
| `failwith`   | function     | Raise the standard `('Failure MSG)`.               |

---

_See also:_ [The Standard Library](../language/stdlib.md) ·
[Collections](../language/collections.md) · [Grammar](grammar.md) ·
[Reserved Identifiers](reserved.md)
