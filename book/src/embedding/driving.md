# Driving the Interpreter

This chapter covers the functions and types that actually run rizz code from
Rust, from the simplest one-shot to a stateful session that grows bindings
across calls.

## One-shot: `parse_and_run`

The simplest entry point parses a source, evaluates every top-level form in
order, and returns the value of the last form together with the final
environment:

```rust,ignore
use rizz::runtime::Value;

let (v, _env) = rizz::parse_and_run(b"(+ 1 2)".as_ref()).unwrap();
assert_eq!(*v, Value::Int(3));

// Forms thread bindings: later forms see earlier let/fn definitions.
let (v, _env) = rizz::parse_and_run(b"(let x 4) (* x x)".as_ref()).unwrap();
assert_eq!(*v, Value::Int(16));
```

The input is any `impl Read`. Use `parse_and_run` for throwaway evaluations — a
single expression, a one-off script. For repeated calls that should share state,
use a `Runtime` (below).

## Bring your own env: `parse_and_run_with_env`

When an outer system already owns the environment — a REPL accumulating bindings,
or a host that wants custom builtins in scope — evaluate against a supplied
`Env` and thread the returned one back in:

```rust,ignore
use rizz::runtime::Value;

let env = rizz::prelude::env();
let (v1, env) = rizz::parse_and_run_with_env(b"(let n 7)".as_ref(), &env).unwrap();
assert_eq!(*v1, Value::Int(7));

// Subsequent calls see `n` because we passed the returned env back in.
let (v2, _env) = rizz::parse_and_run_with_env(b"(* n 6)".as_ref(), &env).unwrap();
assert_eq!(*v2, Value::Int(42));
```

If the `env` you pass doesn't include the prelude, builtins like `+` and `cond`
won't resolve. Build a fresh prelude with `rizz::prelude::env()`, or merge extra
bindings into one with `rizz::prelude::install(...)` (see
[Custom Builtins](builtins.md)).

## Pre-parsed forms: `eval_forms`

If you already hold a `Vec<Sexp>` — from driving the `Parser` yourself, or
constructing forms programmatically — `eval_forms` is the loop the helpers above
sit on:

```rust,ignore
use rizz::{Parser, eval_forms, runtime::Value};

let forms = Parser::new(b"(let a 3) (let b 4) (+ a b)".as_ref()).parse().unwrap();
let env = rizz::prelude::env();
let (v, _env) = eval_forms(forms, &env).unwrap();
assert_eq!(*v, Value::Int(7));
```

## Stateful sessions: `Runtime`

`Runtime` is the recommended entry point for embedders and REPLs. It owns an
`Env` that **grows across calls**, so bindings introduced by one evaluation are
visible to the next — exactly what incremental, user-driven input needs.

```rust,ignore
use rizz::{Runtime, runtime::Value};

let mut rt = Runtime::new();
rt.eval(b"(let x 1)".as_ref()).unwrap();        // binds `x` in the session
rt.eval(b"(let y 2)".as_ref()).unwrap();        // adds `y`; `x` still visible
let v = rt.eval(b"(+ x y)".as_ref()).unwrap();  // sees both
assert_eq!(*v, Value::Int(3));
```

State persists because the runtime threads its growing env through every call.
A counter built from a [ref](../language/refs.md) survives across `eval`s:

```rust,ignore
use rizz::{Runtime, runtime::Value};

let mut rt = Runtime::new();
rt.eval(b"(let counter (ref 0))".as_ref()).unwrap();
rt.eval(b"(set! counter (+ (deref counter) 1))".as_ref()).unwrap();
rt.eval(b"(set! counter (+ (deref counter) 1))".as_ref()).unwrap();
let v = rt.eval(b"(deref counter)".as_ref()).unwrap();
assert_eq!(*v, Value::Int(2));
```

### Loading files: `eval_file`

`eval_file` reads a path, parses it, and anchors the runtime's `base_dir` to the
file's directory so that any relative `(open "...")` inside resolves correctly:

```rust,ignore
use rizz::Runtime;

let mut rt = Runtime::new();
let value = rt.eval_file("examples/main.rz").unwrap();
println!("{value}");
```

I/O failures opening the file surface as `RuntimeError::IOError` — the same
family `(open ...)` uses for a missing module.

### Per-form evaluation: `eval_form`

When the host has already parsed (or constructed) forms, feed them one at a time.
This is what a custom front-end or a structured REPL uses:

```rust,ignore
use rizz::{Parser, Runtime, runtime::Value};
use std::rc::Rc;

let forms = Parser::new(b"(let n 21) (* n 2)".as_ref()).parse().unwrap();
let mut rt = Runtime::new();
let mut last = Rc::new(Value::Unit);
for form in forms {
    last = rt.eval_form(Rc::new(form.into())).unwrap();
}
assert_eq!(*last, Value::Int(42));
```

Note the `form.into()` — that is the **Lower** step, converting a parsed `Sexp`
into the runtime `Value` the evaluator walks.

### Inspecting the session env

`Runtime::env()` returns the current top-level `Env`, useful for inspecting
bindings after a run or handing the env to `parse_and_run_with_env`. The runtime
also pins a _base env_ (a snapshot taken at construction) that seeds every
`(open ...)`d module — which is how host builtins reach loaded modules. That
mechanism is the subject of [Custom Builtins](builtins.md).

---

_See also:_ [Overview](overview.md) · [Working with Values](values.md) ·
[Custom Builtins](builtins.md) · [A Worked Example](worked-example.md)
