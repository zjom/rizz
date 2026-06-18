# Error Handling

Failures from the library come in two families, unified under one top-level enum.
Both implement `std::error::Error` via `thiserror`, so they compose with
`anyhow::Result` and the `?` operator without extra work.

## `RizzError`: parse or runtime

The crate-root error type splits the pipeline cleanly:

```rust,ignore
match rizz::parse_and_run(b"(+ 1)".as_ref()) {
    Ok((value, _env)) => println!("{value}"),
    Err(e) => eprintln!("rizz failed: {e}"),
}
```

```rust,ignore
pub enum RizzError {
    ParseError(rizz::ParseError),       // surface-syntax problems
    RuntimeError(rizz::RuntimeError),   // evaluation problems
}
```

Both variants are `#[from]`-tagged, so `?` threads the underlying error through,
and `Display` delegates to the inner error — you can print a `RizzError`
directly without matching the variant.

Note the split by entry point: `parse_and_run` and `Runtime::eval`/`eval_file`
return `RizzError` (they parse *and* evaluate), while `eval_forms` and
`Runtime::eval_form` already have parsed input and return `RuntimeError`.

## `ParseError`

A `ParseError` is a surface-syntax problem: unbalanced delimiters, a malformed
number, an invalid string escape, a stray `;`, non-UTF-8 bytes, or unexpected
end of input. Every variant carries a `parser::Position` (line, column, byte
offset), so you can underline the offending token in a host editor or REPL.

Because parsing happens entirely before evaluation, a `ParseError` means *nothing
ran*.

## `RuntimeError`

The evaluation-time failures. The variants you'll encounter most:

| Variant            | Meaning                                                        |
| ------------------ | -------------------------------------------------------------- |
| `UnknownIdent`     | A name was looked up and not bound.                            |
| `NotCallable`      | A non-callable value appeared in head position.               |
| `ArityMismatch`    | Wrong number of arguments (carries an `Arity` describing the contract). |
| `TypeMismatch`     | An argument was the wrong type (`expected` vs `got`).         |
| `IndexOob`         | An index outside a collection's bounds.                       |
| `ArithmeticError`  | Integer overflow, divide-by-zero, or a NaN comparison.        |
| `ParseError`       | A runtime parse, e.g. `int-of` on a non-numeric string.      |
| `RecursionLimit`   | Evaluation recursed past the configured cap.                  |
| `InModule`         | A failure inside an `(open ...)`d module (wraps the inner error + path). |
| `Raised`           | A value raised by `(raise ...)` that no `try` caught.        |
| `IOError`          | An I/O failure (a missing file for `open`/`eval_file`).      |
| `Other`            | A host-side error injected via `anyhow`.                     |

These map directly onto the language's [error story](../language/errors.md):
everything except `Raised` is a structural fault that a rizz `try` cannot catch;
`Raised` is the one a program produces and observes on purpose.

### Matching on a variant

Because the variants are public, a host can react to specific failures — for
example, treating an uncaught exception differently from a syntax error:

```rust,ignore
use rizz::{RuntimeError, runtime::Value};

match rt.eval(source) {
    Ok(v) => handle(v),
    Err(rizz::RizzError::RuntimeError(RuntimeError::Raised { value })) => {
        // A deliberate (raise ...) the script didn't catch.
        report_uncaught(value);
    }
    Err(e) => return Err(e.into()),  // anything else: propagate
}
```

`InModule` keeps the underlying structured error intact (boxed), so you can
recurse into `source` to find the root cause and the module path that failed.

## Surfacing host errors from a builtin

A [custom builtin](builtins.md) returns `Result<_, RuntimeError>`. For
application failures that don't fit a structural variant, wrap an
`anyhow::Error` — `RuntimeError` has a `#[from]` for it via the `Other` variant:

```rust,ignore
use rizz::runtime::{NativeFn, Value, RuntimeError};
use std::rc::Rc;

let read_file = NativeFn::pure("read-file".into(), 1, |args| {
    let path = args[0].as_str()
        .ok_or_else(|| RuntimeError::type_mismatch("read-file", "str", &args[0]))?;
    let text = std::fs::read_to_string(&*path)
        .map_err(|e| RuntimeError::Other(e.into()))?;   // host error -> Other
    Ok(Rc::new(Value::Str(text.into())))
});
```

If instead you want the builtin to raise a **catchable** rizz exception, return
`RuntimeError::Raised { value }` with a tagged-cons value, so a script's
`try`/`try-with` can handle it.

## Runaway recursion is contained

User scripts can recurse arbitrarily. Rather than overflow the host thread, the
evaluator raises `RuntimeError::RecursionLimit` once nesting passes a per-thread
cap (default 10,000). Tune it with `rizz::runtime::set_recursion_limit` when your
host runs on unusually small or large stacks. The physical stack is also grown in
segments, so legitimate deep recursion within the cap won't crash — important
because embedders may run on default 2 MiB threads. See
[Performance](../idioms/performance.md).

## Composing with `anyhow`

Since both error types implement `std::error::Error`, the idiomatic host wrapper
is just `anyhow::Result`:

```rust,ignore
fn run_script(src: &[u8]) -> anyhow::Result<()> {
    let (value, _env) = rizz::parse_and_run(src)?;   // RizzError -> anyhow via ?
    println!("{value}");
    Ok(())
}
```

One caveat the CLI itself hits: `RizzError` holds `Rc`s and so is not `Send +
Sync`. If you need to move an error across threads (as `anyhow` sometimes
requires), render it to a `String` first — `anyhow::anyhow!("{e}")`.

---

*See also:* [Custom Builtins](builtins.md) ·
[Errors and Exceptions](../language/errors.md) ·
[Performance](../idioms/performance.md) · [A Worked Example](worked-example.md)
