# Custom Builtins

The whole point of embedding rizz is to give scripts access to *your*
application. You do that by installing **native functions** — Rust closures
exposed to rizz code as callable values. This chapter shows how to write them,
how to choose the right flavor, and how to wire them into a runtime.

## The shape of a native function

A native function is a `rizz::runtime::NativeFn`. The simplest kind, `pure`,
takes the already-evaluated arguments and returns a `Value`:

```rust,ignore
use rizz::{Env, Runtime, runtime::{NativeFn, Value, RuntimeError}};
use std::rc::Rc;

// A pure Rust function: takes evaluated args, returns a Value.
let greet = NativeFn::pure("greet".into(), 1, |args| {
    match args[0].as_str() {
        Some(name) => Ok(Rc::new(Value::Str(format!("hi, {name}!").into()))),
        None => Err(RuntimeError::type_mismatch("greet", "str", &args[0])),
    }
})
.with_doc("(greet NAME)\n\nReturns a greeting string for NAME.".into());

// Merge into the standard prelude, then build a runtime from it.
let env = rizz::prelude::install(
    Env::new().update("greet".into(), Rc::new(Value::NativeFn(Rc::new(greet)))),
);

let mut rt = Runtime::with_env(env);
let v = rt.eval(br#"(greet "world")"#.as_ref()).unwrap();
assert_eq!(v.as_str().as_deref(), Some("hi, world!"));
```

Three things are happening: building the `NativeFn`, **installing** it into an
env alongside the prelude, and constructing a `Runtime` from that env. We'll take
them in turn.

## Choosing a variant: least power needed

A `NativeFn` comes in four flavors. They differ along two axes — does the
function see the environment, and can it change it — plus whether arguments
arrive evaluated. Pick the **first** one that fits; granting more power than you
need costs clarity and a static guarantee.

| Variant     | Sees env? | Returned env propagates? | Args evaluated? | Use for                                     |
| ----------- | :-------: | :----------------------: | :-------------: | ------------------------------------------- |
| `pure`      |    no     |           n/a            |       yes       | `+`, `len`, `car` — operate on args only    |
| `with_env`  |    yes    |            no            |       yes       | higher-order fns that call a callable arg   |
| `impure`    |    yes    |           yes            |       yes       | loader-style fns that extend the caller env |
| `macro_`    |    yes    |            no            |     **no**      | control structures over raw arg forms       |

1. **Operates only on its arguments?** Use `pure`. This is the overwhelming
   majority of builtins.
2. **Needs to invoke a callable passed as an argument** (so it must call back
   into the evaluator via `rizz::runtime::apply`)? Use `with_env`. The env is
   readable, but bindings the callback introduces stay scoped to the call —
   they can't leak. This is how `fmap`, `filter`, `reduce`, and `show` are built.
3. **Genuinely introduces bindings that outlive the call?** Use `impure`; the env
   it returns is threaded back into the caller. (No prelude builtin ships this
   way today; the variant exists for loader/import-style primitives.)
4. **Wants its arguments unevaluated** — i.e. a Rust-implemented macro? Use
   `macro_`. The body receives the raw forms and produces the result directly.

The constructors mirror the table:

```rust,ignore
NativeFn::pure(name, nargs, |args| { ... })          // &[Rc<Value>] -> Result<Rc<Value>>
NativeFn::with_env(name, nargs, |args, env| { ... }) // ... + &Env, returns a value
NativeFn::impure(name, nargs, |args, env| { ... })   // ... returns (value, Env)
NativeFn::macro_(name, nargs, |args, env| { ... })   // args are raw, unevaluated forms
```

## Arity is a lower bound

The `nargs` argument is the **minimum** arity. The runtime guarantees your
closure receives *at least* that many arguments — but allows more. Passing
`nargs = 0` opts out of checking entirely, which is the idiom for variadic
functions. If you need a strict upper bound, check `args.len()` yourself and
return a `RuntimeError::ArityMismatch`.

```rust,ignore
// Exactly one arg expected: declare 1, and (since there's no upper-bound
// check) optionally reject extras by hand if it matters.
let f = NativeFn::pure("negate".into(), 1, |args| {
    let n = args[0].as_int()
        .ok_or_else(|| RuntimeError::type_mismatch("negate", "int", &args[0]))?;
    Ok(Rc::new(Value::Int(-n)))
});
```

## Reporting errors

Builtins return `Result<_, RuntimeError>`. The convention every prelude function
follows is `RuntimeError::type_mismatch(name, expected, &got)`, which fills the
`got` field from the value's `type_name` so messages stay uniform:

```rust,ignore
None => Err(RuntimeError::type_mismatch("greet", "str", &args[0])),
```

For application-specific failures you can surface a host error through
`RuntimeError::Other(anyhow::Error)` — see [Error Handling](errors.md). And if
your builtin should raise a *catchable* rizz exception rather than abort, return
`RuntimeError::Raised { value }` with a tagged-cons value (the same shape
`(raise ...)` produces).

## Attaching documentation

Chain `.with_doc(...)` onto any constructor to give the builtin a doc string,
which rizz code reads back with [`show`](../language/documentation.md):

```rust,ignore
NativeFn::pure("len".into(), 1, |args| { /* ... */ })
    .with_doc("(len COLL)\n\nReturns int: the element count of COLL.".into())
```

## Installing into an environment

A `NativeFn` becomes callable from rizz once it's a `Value::NativeFn` bound in an
`Env`. `Env::update(name, value)` adds a binding; `prelude::install(env)` merges
your bindings on top of a fresh prelude:

```rust,ignore
use rizz::{Env, Runtime, runtime::{NativeFn, Value}};
use std::rc::Rc;

let f = NativeFn::pure("answer".into(), 0, |_| Ok(Rc::new(Value::Int(42))));
let extra = Env::new().update("answer".into(), Rc::new(Value::NativeFn(Rc::new(f))));
let env = rizz::prelude::install(extra);

let mut rt = Runtime::with_env(env);
assert_eq!(*rt.eval(b"(answer)".as_ref()).unwrap(), Value::Int(42));
```

> **Collision rule:** in `install`, *your* bindings win over the prelude's, so you
> can both add new names and override standard ones. To get prelude-wins
> behavior instead, write `rizz::prelude::env().union(your_env)` yourself.

## Your builtins reach loaded modules

`Runtime::with_env` pins a snapshot of the construction-time env as the **base
env**, and every `(open ...)`d [module](../language/modules.md) is seeded with
it. That means host builtins are visible inside loaded modules transparently —
*but* top-level user definitions made through the runtime are not, matching the
rule that modules load against a clean scope. Install everything a module might
need at construction time, not incrementally afterward.

```rust,ignore
use rizz::{Env, Runtime, runtime::{NativeFn, Value}};
use std::rc::Rc;

let f = NativeFn::pure("six".into(), 0, |_| Ok(Rc::new(Value::Int(6))));
let env = rizz::prelude::install(
    Env::new().update("six".into(), Rc::new(Value::NativeFn(Rc::new(f)))),
);
let mut rt = Runtime::with_env(env);
assert_eq!(*rt.eval(b"(* (six) 7)".as_ref()).unwrap(), Value::Int(42));
```

---

*See also:* [Working with Values](values.md) · [Error Handling](errors.md) ·
[Modules](../language/modules.md) · [A Worked Example](worked-example.md)
