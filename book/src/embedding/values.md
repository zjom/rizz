# Working with Values

The currency exchanged between Rust and rizz is `rizz::runtime::Value`. It is
`Clone`, `PartialEq`, `Eq`, and `Hash`, so values compare and can be used as map
keys. It is `Rc`-backed, so cloning is cheap and structural sharing is preserved.

This chapter covers building values from Rust types and inspecting values that
come back out.

## Building values from Rust

The standard `From` / `Into` conversions cover the common cases:

```rust,ignore
use rizz::runtime::Value;

let i: Value = 42i64.into();
let f: Value = 3.14f64.into();
let s: Value = "hi".into();
let b: Value = true.into();                    // booleans encode as Int(1) / Int(0)
let xs: Value = vec![1i64, 2, 3].into();       // produces a Value::Array
let none: Value = Option::<i64>::None.into();  // Value::Unit

assert_eq!(i, Value::Int(42));
assert_eq!(b, Value::Int(1));
```

Two conversions worth calling out, because they reflect language semantics:

- **`bool` → `Int(1)` / `Int(0)`.** rizz has no boolean type; truth is `1` and
  falsity is `0` (see [Values and Types](../language/values.md)).
- **`Option::None` → `Unit`.** rizz's "nothing" is `()`.

For lists (cons chains rather than arrays), the constructors `Value::list_of` and
`Value::cons_of` build a cons-terminated list from an iterator — handy when you
want `(1 2 3)` rather than `[1 2 3]`.

## Inspecting a value

When a `Value` comes back from evaluation, these accessors get you to the Rust
data inside. The `as_*` family returns `Option`, yielding `None` when the value
is a different kind:

| Method               | Returns                       | Notes                                |
| -------------------- | ----------------------------- | ------------------------------------ |
| `as_int()`           | `Option<i64>`                 | `Some` for `Int`.                    |
| `as_float()`         | `Option<f64>`                 | `Some` for `Float`.                  |
| `as_str()`           | `Option<Rc<str>>`             | `Some` for `Str`.                    |
| `as_str_or_ident()`  | `Option<Rc<str>>`             | Accepts a `Str` *or* an `Ident`.     |
| `as_array()`         | `Option<im::Vector<Rc<Value>>>` | `Some` for `Array`.                |
| `as_map()`           | `Option<im::HashMap<…>>`      | `Some` for `Map`.                    |

```rust,ignore
use rizz::runtime::Value;

let (v, _env) = rizz::parse_and_run(br#"(str-upper "hi")"#.as_ref()).unwrap();
assert_eq!(v.as_str().as_deref(), Some("HI"));

let (v, _env) = rizz::parse_and_run(b"(+ 1 2)".as_ref()).unwrap();
assert_eq!(v.as_int(), Some(3));
```

`.as_deref()` on the `Option<Rc<str>>` gives you an `Option<&str>` for easy
comparison, as in the snippet above.

### Predicates and the type name

- `is_truthy()` — the language's [truthiness rule](../language/values.md):
  `Unit`, `0`, `0.0`, `""`, the empty ident, `[]`, `{}`, and any ref whose
  contents are falsy are `false`; everything else (including all closures) is
  `true`.
- `is_callable()`, `is_unit()`, `is_numeric()` — quick variant checks.
- `Value::type_name(&v)` — the variant name as a `&'static str` (`"int"`,
  `"str"`, `"cons"`, …), the same string `(typeof v)` reflects and that error
  messages use.

```rust,ignore
use rizz::runtime::Value;

let v = Value::Int(0);
assert!(!v.is_truthy());
assert_eq!(Value::type_name(&v), "int");
```

## Walking a list

`Value::iter(&Rc<Value>)` walks a cons list, yielding each element. A non-cons
value yields itself once — which is the same "scalar or list" convenience rizz's
own iteration builtins use:

```rust,ignore
use rizz::{runtime::Value};
use std::rc::Rc;

let (v, _env) = rizz::parse_and_run(b"'(1 2 3)".as_ref()).unwrap();
let nums: Vec<i64> = Value::iter(&v).filter_map(|x| x.as_int()).collect();
assert_eq!(nums, vec![1, 2, 3]);
```

## Formatting for output

Two formatters, matching the language's `to-str` and `repr` behaviors:

- `display()` — what `(to-str v)` uses; top-level strings are unquoted.
- `repr()` — quotes strings so nested collections stay readable.

`Value` also implements `Display`, so `println!("{value}")` works directly — it
uses the `display` formatting, which is what the CLI prints for a program's
result.

```rust,ignore
let (v, _env) = rizz::parse_and_run(br#"["a" "b"]"#.as_ref()).unwrap();
println!("{v}");          // [a b]  via Display/display
// v.repr() would render with quotes: ["a" "b"]
```

## Collections are `im` containers

Arrays are `im::Vector<Rc<Value>>` and maps are `im::HashMap<Rc<Value>,
Rc<Value>>`. They are persistent: cloning is cheap, and the "modifying" builtins
return structurally-shared copies. If you pull an `as_array()` out and mutate
your copy, the value inside the interpreter is unaffected.

---

*See also:* [Driving the Interpreter](driving.md) · [Custom Builtins](builtins.md) ·
[Values and Types](../language/values.md) · [A Worked Example](worked-example.md)
