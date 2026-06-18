# Modules

A rizz program can grow past one file. The module system is three special forms
— `open`, `load`, and `load-quoted` — that load a sibling source file and do
different things with the result. There is no separate "module" declaration: any
`.rz` file is loadable, and its top-level bindings _are_ its exports.

## The three loaders

```clojure
(open PATH)
(open PATH PREFIX)
(load PATH)
(load-quoted PATH)
```

All three read the file at `PATH` and (for `open`/`load`) evaluate its top-level
forms in a fresh **module environment**. They differ in what they hand back:

- **`open`** — _merges_ all of the module's top-level bindings into the caller's
  environment, and returns the value of the module's last form.
- **`load`** — merges nothing; returns the module's bindings as a **map** keyed
  by identifier, so you can inspect or destructure the module as a value.
- **`load-quoted`** — does _not_ evaluate the file; returns its top-level forms
  as **unevaluated data** (a list of S-expressions), for metaprogramming.

`PATH` may be a string (`"math"`) or a bare identifier that spells a valid
filename (`math`). Any other type is a `TypeMismatch`.

## A two-file example

```clojure
;; mod.rz
(let answer 42)
(let _secret 7)
(fn dbl (x) (* x 2))
```

```clojure
;; caller.rz
(open "mod")       ;; merges answer, _secret, and dbl into scope
(dbl answer)       ;; => 84
_secret            ;; => 7   — open leaks "private" bindings too (see below)
```

### `open` with a prefix

Pass a second identifier to **namespace** every imported binding as
`PREFIX.NAME`, which keeps a module's names from colliding with yours. The
prefix is taken literally — it is not evaluated:

```clojure
(open "mod" m)     ;; binds m.answer, m._secret, m.dbl
(m.dbl m.answer)   ;; => 84
```

### `load` returns a map

```clojure
(let mod (load "mod"))   ;; => { answer : 42  _secret : 7  dbl : <fn> }
(get mod 'answer)        ;; => 42
```

`load` is the right tool when you want the module as first-class data — to pick a
few bindings out of it, or to choose between modules at runtime.

### `load-quoted` returns forms

```clojure
(load-quoted "mod")
;; => ((let answer 42) (let _secret 7) (fn dbl (x) (* x 2)))
```

This hands you the parsed-but-unevaluated source, for tooling and
[metaprogramming](macros.md).

## Path resolution

- If `PATH` has no extension, `.rz` is appended.
- A **relative** path resolves against the directory of the file doing the
  loading — its _anchor_. The entry point sets the initial anchor (to the script
  file's directory), and every `open` re-anchors to the opened file's directory.
  With no anchor, the process working directory is used.
- An **absolute** path is used verbatim.

The re-anchoring matters: a module can `(open "sibling")` and have it resolve
relative to _that module_, not relative to whoever loaded it. Modules stay
portable regardless of who imports them.

## The module environment

A loaded module evaluates against a **fresh copy of the prelude** — so `+`,
`cond`, and the rest are available — but **not** against the caller's top-level
definitions. `open` always loads against a clean scope. A name you defined in
the caller before calling `open` is invisible inside the module.

(When you embed rizz in Rust, host builtins you install _are_ visible to loaded
modules, because the runtime seeds modules with the base environment. See
[Custom Builtins](../embedding/builtins.md).)

## What `open` leaks back

- **Every** top-level binding the module introduced becomes visible in the
  caller — including `_`-prefixed ones. The `_` prefix is a _naming convention_
  for "private"; `open` does not enforce it. If you want true namespacing, use a
  `PREFIX`.
- On a name collision, the **module's** binding wins (it overwrites the
  caller's).
- The caller's anchor is preserved across the call.

`load` and `load-quoted` leak nothing — they hand back a value instead.

## Errors while loading

A failure inside a loaded module — a parse error, a runtime error, or an
uncaught [exception](errors.md) — surfaces wrapped as an `InModule` error that
names the module's path. Importantly, an exception raised during a module load
is **not** caught by a `try` in the importing file; catching is limited to a
raise within the same evaluation.

---

_See also:_ [The Evaluation Model](evaluation.md) ·
[Errors and Exceptions](errors.md) · [Custom Builtins](../embedding/builtins.md) ·
_SPEC.md_ §8
