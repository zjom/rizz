//! Tests for the `open` / `load` / `load-quoted` special forms: file loading
//! semantics — extension defaulting, relative-path anchoring against the
//! caller's source directory, full (unfiltered) binding merge, prefix
//! namespacing, the map/list results of `load`/`load-quoted`, and the
//! surfaced error shape.

use rizz::runtime::{Arity, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Per-test scratch directory under the OS temp dir. Cleaned up on drop so a
/// panicking assertion still removes the files; the unique suffix combines pid
/// with a process-local counter so parallel `cargo test` workers never collide.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rizz-open-{}-{}-{}", std::process::id(), n, tag));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let p = self.0.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&p, contents).expect("write module file");
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(src: &str) -> Rc<Value> {
    rizz::parse_and_run(src.as_bytes())
        .map(|(v, _)| v)
        .unwrap_or_else(|e| panic!("eval of `{src}` failed: {e}"))
}

/// Path source needs single backslashes on Windows escaped for the rizz string
/// literal. On Unix this is a no-op.
fn quote_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "\\\\");
    format!("\"{s}\"")
}

// ----- happy path: return value and binding leak -----

#[test]
fn open_returns_value_of_last_form() {
    let tmp = TempDir::new("last-form");
    let m = tmp.write("m.rz", "(let x 1) (+ 41 1)");
    let src = format!("(open {})", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(42));
}

#[test]
fn open_leaks_public_bindings_into_caller() {
    let tmp = TempDir::new("leak-pub");
    let m = tmp.write("m.rz", "(let answer 42) (fn dbl (x) (* x 2))");
    // After opening, both `answer` and `dbl` are visible to the next form.
    let src = format!("(open {}) (dbl answer)", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(84));
}

#[test]
fn open_merges_private_underscore_bindings() {
    let tmp = TempDir::new("private");
    // `open` merges everything regardless of `_` — `_secret` leaks too.
    let m = tmp.write("m.rz", "(let _secret 7) (let public 1)");
    let secret = format!("(open {}) _secret", quote_path(&m));
    assert_eq!(*run(&secret), Value::Int(7));

    let visible = format!("(open {}) public", quote_path(&m));
    assert_eq!(*run(&visible), Value::Int(1));
}

// ----- open with a prefix ident namespaces the merged bindings -----

#[test]
fn open_with_prefix_namespaces_bindings() {
    let tmp = TempDir::new("prefix");
    let m = tmp.write("m.rz", "(let answer 42) (fn dbl (x) (* x 2))");
    // `(open PATH PREFIX)` rewrites every name to `PREFIX.NAME`.
    let src = format!("(open {} m) (m.dbl m.answer)", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(84));
}

#[test]
fn open_with_prefix_does_not_bind_unprefixed_names() {
    let tmp = TempDir::new("prefix-only");
    let m = tmp.write("m.rz", "(let answer 42)");
    // The bare `answer` must not be bound when a prefix is supplied.
    let src = format!("(open {} m) answer", quote_path(&m));
    assert!(rizz::parse_and_run(src.as_bytes()).is_err());
}

#[test]
fn open_with_non_ident_prefix_is_type_error() {
    let tmp = TempDir::new("prefix-bad");
    let m = tmp.write("m.rz", "1");
    // The prefix slot must be an ident, not a string.
    let src = format!("(open {} \"m\")", quote_path(&m));
    let err = rizz::parse_and_run(src.as_bytes()).expect_err("type error");
    assert!(matches!(
        err,
        rizz::RizzError::RuntimeError(rizz::RuntimeError::TypeMismatch { .. })
    ));
}

// ----- load returns the module's bindings as a map -----

#[test]
fn load_returns_bindings_as_map() {
    let tmp = TempDir::new("load-map");
    let m = tmp.write("m.rz", "(let answer 42) (let _secret 7)");
    // `load` reifies the bindings; nothing leaks, so we look them up in the map.
    let src = format!("(let mod (load {})) (get mod 'answer)", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(42));

    // It includes `_`-prefixed names too.
    let src = format!("(let mod (load {})) (get mod '_secret)", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(7));
}

#[test]
fn load_map_holds_only_module_bindings_not_the_prelude() {
    let tmp = TempDir::new("load-map-size");
    let m = tmp.write("m.rz", "(let a 1) (let b 2) (let c 3)");
    // The seeded prelude is diffed back out: only the module's own bindings
    // remain, so the map has exactly three keys.
    let src = format!("(len (keys (load {})))", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(3));
}

#[test]
fn open_with_prefix_does_not_namespace_the_prelude() {
    let tmp = TempDir::new("prefix-no-prelude");
    let m = tmp.write("m.rz", "(fn dbl (x) (* x 2))");
    // Only the module's `dbl` is prefixed; prelude names are not, so `m.+`
    // is unbound.
    let src = format!("(open {} m) m.+", quote_path(&m));
    assert!(rizz::parse_and_run(src.as_bytes()).is_err());
}

#[test]
fn load_does_not_leak_bindings_into_caller() {
    let tmp = TempDir::new("load-noleak");
    let m = tmp.write("m.rz", "(let answer 42)");
    // Unlike `open`, `load` merges nothing — `answer` stays unbound.
    let src = format!("(load {}) answer", quote_path(&m));
    assert!(rizz::parse_and_run(src.as_bytes()).is_err());
}

// ----- load-quoted returns the file's forms as data -----

#[test]
fn load_quoted_returns_forms_as_data() {
    let tmp = TempDir::new("load-quoted");
    let m = tmp.write("m.rz", "(let x 1) (+ 41 1)");
    // The forms come back unevaluated as a list; the second is `(+ 41 1)`,
    // which we can pull out by index and `eval` to 42.
    let src = format!("(eval (get (load-quoted {}) 1))", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(42));
}

#[test]
fn load_quoted_does_not_evaluate() {
    let tmp = TempDir::new("load-quoted-noeval");
    // A file that would error if evaluated still loads fine as data.
    let m = tmp.write("m.rz", "(car 5)");
    let src = format!("(len (load-quoted {}))", quote_path(&m));
    assert_eq!(*run(&src), Value::Int(1));
}

// ----- extension defaulting -----

#[test]
fn open_appends_rz_extension_when_missing() {
    let tmp = TempDir::new("ext-default");
    tmp.write("m.rz", "123");
    // Strip the `.rz` so the special form has to append it.
    let stem = tmp.path().join("m");
    let src = format!("(open {})", quote_path(&stem));
    assert_eq!(*run(&src), Value::Int(123));
}

#[test]
fn open_preserves_existing_extension() {
    let tmp = TempDir::new("ext-keep");
    // Non-default extension is honored verbatim — `open` only fills the gap.
    let p = tmp.write("m.lisp", "456");
    let src = format!("(open {})", quote_path(&p));
    assert_eq!(*run(&src), Value::Int(456));
}

// ----- relative path resolution against base_dir -----

#[test]
fn open_resolves_relative_path_against_base_dir() {
    let tmp = TempDir::new("base-dir");
    tmp.write("mod.rz", "(let v 10)");

    // Caller env has its base_dir pinned to the temp dir, mimicking how the
    // CLI sets it from the entrypoint script's directory.
    let env = rizz::prelude::env().with_base_dir(Some(tmp.path().to_path_buf()));
    let src = "(open \"mod\") v";
    let (v, _) = rizz::parse_and_run_with_env(src.as_bytes(), &env).expect("eval");
    assert_eq!(*v, Value::Int(10));
}

#[test]
fn nested_open_resolves_relative_to_opened_files_directory() {
    // `outer.rz` lives in tmp/; it opens `inner` (a sibling) by relative name.
    // The inner load must succeed because `open` re-anchors the child env's
    // base_dir to the opened file's parent — not the original caller's.
    let tmp = TempDir::new("nested");
    tmp.write("inner.rz", "(let v 77)");
    let outer = tmp.write("outer.rz", "(open \"inner\") v");

    // The top-level caller has NO base_dir, so a bare "inner" would fail from
    // the process CWD. It only works because outer.rz's directory becomes the
    // child env's anchor.
    let src = format!("(open {})", quote_path(&outer));
    assert_eq!(*run(&src), Value::Int(77));
}

// ----- error paths -----

#[test]
fn open_with_no_args_is_arity_error() {
    let err = rizz::parse_and_run("(open)".as_bytes()).expect_err("arity error");
    assert!(matches!(
        err,
        rizz::RizzError::RuntimeError(rizz::RuntimeError::ArityMismatch {
            expected: Arity::Range(1, 2),
            got: 0,
            ..
        })
    ));
}

#[test]
fn open_with_three_args_is_arity_error() {
    let err = rizz::parse_and_run("(open \"a\" b c)".as_bytes()).expect_err("arity error");
    assert!(matches!(
        err,
        rizz::RizzError::RuntimeError(rizz::RuntimeError::ArityMismatch {
            expected: Arity::Range(1, 2),
            got: 3,
            ..
        })
    ));
}

#[test]
fn open_with_non_string_path_is_type_error() {
    let err = rizz::parse_and_run("(open 5)".as_bytes()).expect_err("type error");
    assert!(matches!(
        err,
        rizz::RizzError::RuntimeError(rizz::RuntimeError::TypeMismatch { .. })
    ));
}

#[test]
fn open_missing_file_surfaces_io_error() {
    let tmp = TempDir::new("missing");
    let p = tmp.path().join("does-not-exist.rz");
    let src = format!("(open {})", quote_path(&p));
    let err = rizz::parse_and_run(src.as_bytes()).expect_err("io error");
    assert!(matches!(
        err,
        rizz::RizzError::RuntimeError(rizz::RuntimeError::IOError(_))
    ));
}

// ----- path may be supplied as an identifier (symbol) -----

#[test]
fn open_accepts_ident_path() {
    // `as_str_or_ident` lets a bare symbol stand in for the path string when
    // it happens to spell a valid filename — useful in scripts that prefer
    // `(open my-module)` over `(open "my-module")`.
    let tmp = TempDir::new("ident-path");
    tmp.write("modname.rz", "(let x 9)");
    let env = rizz::prelude::env().with_base_dir(Some(tmp.path().to_path_buf()));
    let (v, _) = rizz::parse_and_run_with_env(b"(open 'modname) x" as &[u8], &env).expect("eval");
    assert_eq!(*v, Value::Int(9));
}

// ----- module errors stay structured -----

#[test]
fn runtime_error_inside_module_surfaces_as_in_module() {
    let tmp = TempDir::new("inmodule-runtime");
    let p = tmp.write("bad.rz", "(car 5)");
    let src = format!("(open {})", quote_path(&p));
    let err = rizz::parse_and_run(src.as_bytes()).expect_err("module error");
    match err {
        rizz::RizzError::RuntimeError(rizz::RuntimeError::InModule { path, source }) => {
            assert!(path.ends_with("bad.rz"), "path was {path:?}");
            // The inner error is still matchable, not a stringified blob.
            assert!(matches!(
                *source,
                rizz::RizzError::RuntimeError(rizz::RuntimeError::TypeMismatch { .. })
            ));
        }
        other => panic!("expected InModule, got {other:?}"),
    }
}

#[test]
fn parse_error_inside_module_surfaces_as_in_module() {
    let tmp = TempDir::new("inmodule-parse");
    let p = tmp.write("bad.rz", "(let x"); // unterminated
    let src = format!("(open {})", quote_path(&p));
    let err = rizz::parse_and_run(src.as_bytes()).expect_err("module error");
    match err {
        rizz::RizzError::RuntimeError(rizz::RuntimeError::InModule { source, .. }) => {
            assert!(matches!(
                *source,
                rizz::RizzError::ParseError(rizz::ParseError::UnexpectedEof { .. })
            ));
        }
        other => panic!("expected InModule, got {other:?}"),
    }
}
