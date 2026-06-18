# A Worked Example

Let's tie the embedding chapters together by using rizz as a **scripting and
configuration layer** for a Rust application. The host will:

1. expose a couple of its own functions to rizz,
2. evaluate a user-supplied `.rz` script,
3. read the script's result back into Rust as ordinary data.

This is the canonical embedding shape — the language gives users a safe, small
surface to express logic, and the host controls exactly what they can touch.

## The script

Suppose users configure our app with a rizz file. They have the full language —
`let`, `if`, `cond`, arithmetic, collections — plus whatever host functions we
choose to expose. Here we give them one host builtin, `scale`, and let them
return a settings map:

```clojure
;; config.rz
(let base-workers 4)

;; `scale` is provided by the host (Rust), not the prelude
(let workers (scale base-workers))

{ "workers" : workers
  "verbose" : (> workers 8)
  "name"    : (str-upper "service") }
```

## The host

The host installs `scale`, builds a `Runtime`, evaluates the script, and pulls
the resulting map apart.

```rust,ignore
use rizz::{Env, Runtime, runtime::{NativeFn, Value, RuntimeError}};
use std::rc::Rc;

fn main() -> anyhow::Result<()> {
    // 1. A host builtin. Imagine this reads the machine's CPU count;
    //    here it just multiplies by a fixed factor.
    let scale = NativeFn::pure("scale".into(), 1, |args| {
        let n = args[0].as_int()
            .ok_or_else(|| RuntimeError::type_mismatch("scale", "int", &args[0]))?;
        Ok(Rc::new(Value::Int(n * 3)))
    })
    .with_doc("(scale N)\n\nMultiplies N by the host scaling factor.".into());

    // 2. Install it alongside the standard prelude, then build a runtime.
    //    The pinned base env makes `scale` reachable from (open ...)d modules too.
    let env = rizz::prelude::install(
        Env::new().update("scale".into(), Rc::new(Value::NativeFn(Rc::new(scale)))),
    );
    let mut rt = Runtime::with_env(env);

    // 3. Evaluate the script. `eval_file` anchors relative (open ...) paths
    //    to the file's directory.
    let result = rt.eval_file("config.rz")?;

    // 4. Read the result back into Rust.
    let map = result.as_map()
        .ok_or_else(|| anyhow::anyhow!("config must evaluate to a map"))?;

    let workers = map.get(&Rc::new(Value::Str("workers".into())))
        .and_then(|v| v.as_int())
        .unwrap_or(1);
    let verbose = map.get(&Rc::new(Value::Str("verbose".into())))
        .map(|v| v.is_truthy())
        .unwrap_or(false);
    let name = map.get(&Rc::new(Value::Str("name".into())))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    println!("workers={workers} verbose={verbose} name={name}");
    // With base-workers = 4: scale -> 12, so:
    // workers=12 verbose=1 name=SERVICE
    Ok(())
}
```

A few things to notice, each pointing back at an earlier chapter:

- **`scale` is `pure`** — it only needs its argument, so it gets the least-power
  variant ([Custom Builtins](builtins.md)).
- **`install` then `with_env`** — your builtin is merged on top of the prelude,
  and the runtime pins it as the base env so loaded modules can see it too.
- **Reading the map** uses `as_map`, `as_int`, `as_str`, and `is_truthy` from
  [Working with Values](values.md). Map keys are `Rc<Value>`, so we build a
  `Value::Str` key to look one up. `is_truthy` honors rizz's rule, so the rizz
  `(> workers 8)` (which yields `1`) reads back as `true`.
- **Errors compose with `anyhow`** — `?` threads a `RizzError` straight through
  ([Error Handling](errors.md)).

## A REPL-shaped variant

If instead of a file you're feeding the runtime incremental input — a REPL, a
network protocol, an editor — keep one `Runtime` alive and call `eval` per input.
Bindings and refs persist across calls:

```rust,ignore
use rizz::{Runtime, runtime::Value};

let mut rt = Runtime::new();
for line in inputs {                 // inputs: impl Iterator<Item = &[u8]>
    match rt.eval(line) {
        Ok(v) => println!("{v}"),    // Display = the language's to-str
        Err(e) => eprintln!("error: {e}"),
    }
}
```

Each `eval` threads the growing environment forward, which is exactly what makes
a session feel stateful (see [Driving the Interpreter](driving.md)).

## Where to go from here

- Expose more of your app through `pure` builtins; reach for `with_env` only when
  a builtin must call a rizz callable you were handed.
- Split user scripts into [modules](../language/modules.md) and let them
  `(open ...)` each other — your host builtins ride along via the base env.
- Sandbox by _omission_: a script can only do what your installed builtins
  permit, so simply not exposing file or network functions keeps scripts pure.

That is the whole embedding story — a small language, your functions, and values
flowing cleanly across the boundary.

---

_See also:_ [Driving the Interpreter](driving.md) · [Custom Builtins](builtins.md) ·
[Working with Values](values.md) · [Error Handling](errors.md)
