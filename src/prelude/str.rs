//! String builtins. Names use a `str-` prefix; `to-str` stringifies any value.

use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![
        ("to-str", to_str()),
        ("str-upper", str_upper()),
        ("str-lower", str_lower()),
        ("str-trim", str_trim()),
    ])
}

/// `(to-str v)`: stringifies any value via [`Value::display`].
fn to_str() -> NativeFn {
    NativeFn::pure("to-str".into(), 1, |args| {
        Ok(Rc::new(Value::Str(args[0].display().into())))
    })
}

/// Reads `args[0]` as a string (accepts `Str`/`Ident`), erroring otherwise.
fn arg_str(name: &str, v: &Rc<Value>) -> Result<Rc<str>, RuntimeError> {
    v.as_str().ok_or_else(|| RuntimeError::type_mismatch(name, "str", v))
}

/// `(str-upper s)`: uppercased copy.
fn str_upper() -> NativeFn {
    NativeFn::pure("str-upper".into(), 1, |args| {
        let s = arg_str("str-upper", &args[0])?;
        Ok(Rc::new(Value::Str(s.to_uppercase().into())))
    })
}

/// `(str-lower s)`: lowercased copy.
fn str_lower() -> NativeFn {
    NativeFn::pure("str-lower".into(), 1, |args| {
        let s = arg_str("str-lower", &args[0])?;
        Ok(Rc::new(Value::Str(s.to_lowercase().into())))
    })
}

/// `(str-trim s)`: leading/trailing whitespace removed.
fn str_trim() -> NativeFn {
    NativeFn::pure("str-trim".into(), 1, |args| {
        let s = arg_str("str-trim", &args[0])?;
        Ok(Rc::new(Value::Str(s.trim().into())))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RispError;

    fn run(src: &str) -> Result<Rc<Value>, RispError> {
        crate::parse_and_run(src.as_bytes()).map(|(v, _)| v)
    }
    fn run_ok(src: &str) -> Rc<Value> {
        run(src).expect("expected successful eval")
    }

    #[test]
    fn to_str_stringifies() {
        assert_eq!(*run_ok("(to-str 42)"), Value::Str("42".into()));
        assert_eq!(*run_ok("(to-str \"hi\")"), Value::Str("hi".into()));
        assert_eq!(*run_ok("(to-str [1 2])"), Value::Str("[1 2]".into()));
    }

    #[test]
    fn upper_lower_trim() {
        assert_eq!(*run_ok("(str-upper \"hi\")"), Value::Str("HI".into()));
        assert_eq!(*run_ok("(str-lower \"HI\")"), Value::Str("hi".into()));
        assert_eq!(*run_ok("(str-trim \"  hi  \")"), Value::Str("hi".into()));
    }

    #[test]
    fn str_upper_rejects_non_str() {
        assert!(matches!(
            run("(str-upper 5)"),
            Err(RispError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }
}
