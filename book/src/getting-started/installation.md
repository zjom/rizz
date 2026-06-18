# Installation & the CLI

rizz is primarily a **library**, but it also ships an optional command-line
binary and REPL behind the `cli` feature. This chapter gets you to a prompt
where you can evaluate rizz code; the [Embedding](../embedding/overview.md) part
covers using it as a library.

## As a library dependency

Add the crate to your `Cargo.toml`. The embeddable interpreter is the default;
the CLI is opt-in:

```toml
[dependencies]
rizz = "0.7"
```

Or, if you want the `rizz` binary and REPL as well:

```toml
[dependencies]
rizz = { version = "0.7", features = ["cli"] }
```

The `cli` feature pulls in `clap` and `rustyline`. Library consumers who only
want to embed the interpreter do not need it.

## Building the CLI from source

Cloning the repository and building with the `cli` feature gives you the `rizz`
binary:

```console
$ cargo build --features cli            # builds the `rizz` binary + repl
$ cargo run --features cli -- --help    # see the options
```

In the examples throughout this book, anywhere you see a prompt you can run the
same code with `cargo run --features cli --` (note the trailing `--`, which
separates Cargo's arguments from the program's).

## The three ways to run code

The CLI takes its source from one of three places.

### 1. Piped standard input

With nothing piped in and no file, the CLI reads from stdin. This is the
quickest way to try a snippet:

```console
$ echo '(+ 1 2)' | cargo run --features cli --
3
```

The program prints the value of the **last** top-level form.

### 2. A file

Pass `-f` (or `--file`) to evaluate a script. By convention rizz files use the
`.rz` extension:

```console
$ cargo run --features cli -- -f script.rz
```

`eval` is the default subcommand, so `-f script.rz` and `eval -f script.rz` are
the same. Loading a file also anchors relative [`open`](../language/modules.md)
paths to that file's directory.

### 3. The interactive REPL

Pass `-i` (or `--interactive`) to start a read-eval-print loop. This needs a
real terminal (a tty):

```console
$ cargo run --features cli -- -i
> (let x 21)
21
> (* x 2)
42
```

Bindings persist across lines within a REPL session, because the REPL threads
one growing environment through every input — exactly the behavior described in
[The Evaluation Model](../language/evaluation.md).

You can also combine `-i` with a file or stdin to **preload** definitions and
then drop into a REPL with them in scope.

## Inspecting the parse tree

The `parse` subcommand parses without evaluating and prints the resulting
S-expressions — handy for understanding how the reader sees your source, or for
debugging a syntax error:

```console
$ echo '(+ 1 (* 2 3))' | cargo run --features cli -- parse
$ echo '[1 2 3]' | cargo run --features cli -- parse --pretty   # multi-line
```

This is the same `Parser` the library exposes; see
[Parsing without evaluating](../embedding/overview.md) in the embedding part.

## A note on output

When you run a program (rather than the REPL), the CLI prints the final value
using rizz's display formatting. Strings print without quotes at the top level;
nested strings inside a collection are quoted so the structure stays readable.
The difference between these two formattings — `display` and `repr` — is covered
in [Working with Values](../embedding/values.md).

---

*See also:* [Your First Program](first-program.md) ·
[The Evaluation Model](../language/evaluation.md) ·
[Embedding Overview](../embedding/overview.md)
