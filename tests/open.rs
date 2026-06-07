//! Tests for the `open` special form: file loading semantics — extension
//! defaulting, relative-path anchoring against the caller's source directory,
//! `_`-private filtering of leaked bindings, and the surfaced error shape.

use rizz::runtime::Value;
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
        let path = std::env::temp_dir().join(format!(
            "rizz-open-{}-{}-{}",
            std::process::id(),
            n,
            tag
        ));
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
fn open_filters_private_underscore_bindings() {
    let tmp = TempDir::new("private");
    // `_secret` must not leak — referencing it after the open should error.
    let m = tmp.write("m.rz", "(let _secret 7) (let public 1)");
    let leaks = format!("(open {}) _secret", quote_path(&m));
    assert!(rizz::parse_and_run(leaks.as_bytes()).is_err());

    // The non-underscore binding is still visible.
    let visible = format!("(open {}) public", quote_path(&m));
    assert_eq!(*run(&visible), Value::Int(1));
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
            expected: 1,
            got: 0,
            ..
        })
    ));
}

#[test]
fn open_with_two_args_is_arity_error() {
    let err = rizz::parse_and_run("(open \"a\" \"b\")".as_bytes()).expect_err("arity error");
    assert!(matches!(
        err,
        rizz::RizzError::RuntimeError(rizz::RuntimeError::ArityMismatch {
            expected: 1,
            got: 2,
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
