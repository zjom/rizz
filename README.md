# rizz

rizz is a small, dynamically typed lisp.

this crate contains the parser/lexer and runtime used by [rizzler](https://github.com/zjom/rizzler)

## documentation

- **[The rizz Programming Language](book/)** — a hands-on guide covering syntax,
  usage, idioms, and how to embed rizz in a Rust application. Built with
  [mdbook](https://rust-lang.github.io/mdBook/):

  ```bash
  mdbook serve book      # live-reloading preview at http://localhost:3000
  mdbook build book      # render to target/book
  ```

  Pushes to `main` publish it to GitHub Pages via
  `.github/workflows/mdbook.yml` (set the repo's Pages source to "GitHub
  Actions" once, under Settings → Pages).
- **[SPEC.md](SPEC.md)** — the formal language specification; the source of
  truth for behavior.
