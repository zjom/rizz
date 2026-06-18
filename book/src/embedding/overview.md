# Embedding Overview

rizz is built to be embedded. The crate exposes the interpreter as a Rust library
so a host application can run rizz code, exchange values with it, and install its
own functions. This part of the book is the Rust-side companion to the language
chapters.

> The Rust snippets here mirror the crate's own doc-tested examples. The exact,
> always-green versions live in the library's API docs (`cargo doc --open`) and
> are exercised by `cargo test --doc`.

## Add the dependency

```toml
[dependencies]
rizz = "0.7"
```

That gives you the embeddable interpreter. The `cli` feature (off by default)
adds the `rizz` binary and REPL; library consumers don't need it.

## The crate at a glance

The library is organized around the three-stage pipeline from the
[Introduction](../introduction.md), each in its own module:

| Stage        | Module          | Output                               |
| ------------ | --------------- | ------------------------------------ |
| **Parse**    | `rizz::parser`  | `Sexp` forms, each with a `Position` |
| **Evaluate** | `rizz::runtime` | `Value` + an updated `Env`           |
| **Builtins** | `rizz::prelude` | The default `Env`                    |

For convenience the most-used types are re-exported at the crate root: `Parser`,
`ParseError`, `Env`, `Runtime`, and `RuntimeError`.

Most embedders never touch the stages directly — the helpers `parse_and_run` and
`Runtime` wire them together.

## Choosing an entry point

| Use case                                          | Entry point              |
| ------------------------------------------------- | ------------------------ |
| Run a string, get the last value back             | `parse_and_run`          |
| Run a string against caller-supplied bindings     | `parse_and_run_with_env` |
| Evaluate already-parsed forms                     | `eval_forms`             |
| Repeated calls that share state (REPL, file load) | `Runtime`                |
| Load and evaluate a `.rz` file                    | `Runtime::eval_file`     |
| Just parse, no evaluation                         | `Parser`                 |

The next chapter, [Driving the Interpreter](driving.md), works through each of
these. The short version: use `parse_and_run` for a one-shot, and a `Runtime`
for anything stateful.

## Hello, embedding

```rust,ignore
use rizz::runtime::Value;

// One-shot: parse, evaluate, take the final value.
let (value, _env) = rizz::parse_and_run(b"(+ 1 (* 2 3))".as_ref()).unwrap();
assert_eq!(*value, Value::Int(7));
```

Every input is `impl std::io::Read`, so a `&[u8]`, a `File`, or stdin all work
without ceremony. The result is an `Rc<Value>` plus the final `Env`.

## Parsing without evaluating

When you want the syntax tree but not the runtime — for tooling, linting, or
feeding into your own evaluator — drive the `Parser` directly:

```rust,ignore
use rizz::Parser;

let mut p = Parser::new(b"(+ 1 2)".as_ref());
let forms = p.parse().unwrap();
assert_eq!(forms.len(), 1);

// Every ParseError carries a position you can underline.
let err = Parser::new(b"(1 2".as_ref()).parse().unwrap_err();
eprintln!("parse failed: {err}");
```

Empty (or comment-only) input is a `ParseError`.

## What's in the rest of this part

- **[Driving the Interpreter](driving.md)** — the entry points in depth, and the
  stateful `Runtime`.
- **[Working with Values](values.md)** — converting between Rust types and
  `Value`, and inspecting results.
- **[Custom Builtins](builtins.md)** — exposing your own Rust functions to rizz
  code via `NativeFn`.
- **[Error Handling](errors.md)** — the error types and how they compose with
  `anyhow`.
- **[A Worked Example](worked-example.md)** — a complete host integration.

---

_See also:_ [Driving the Interpreter](driving.md) ·
[Working with Values](values.md) · [Custom Builtins](builtins.md)
