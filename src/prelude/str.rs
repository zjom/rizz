//! String builtins. Names use a `str-` prefix; `to-str` stringifies any value.

use im::Vector;
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

pub fn env() -> Env {
    Env::of_builtins(vec![
        ("to-str", to_str()),
        ("str-upper", str_upper()),
        ("str-lower", str_lower()),
        ("str-trim", str_trim()),
        ("str-split", str_split()),
        ("str-join", str_join()),
        ("str-replace", str_replace()),
        ("str->int", str_to_int()),
    ])
}

/// `(to-str v)`: stringifies any value via [`Value::display`].
fn to_str() -> NativeFn {
    NativeFn::pure("to-str".into(), 1, |args| {
        Ok(Rc::new(Value::Str(args[0].display().into())))
    })
}

/// Reads `args[0]` as a string, erroring otherwise.
fn arg_str(name: &str, v: &Rc<Value>) -> Result<Rc<str>, RuntimeError> {
    v.as_str()
        .ok_or_else(|| RuntimeError::type_mismatch(name, "str", v))
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

/// `(str-split s sep)`: splits `s` on `sep` into an array of strings. An empty
/// `sep` splits into individual characters.
fn str_split() -> NativeFn {
    NativeFn::pure("str-split".into(), 2, |args| {
        let s = arg_str("str-split", &args[0])?;
        let sep = arg_str("str-split", &args[1])?;
        let parts: Vector<Rc<Value>> = if sep.is_empty() {
            s.chars()
                .map(|c| Rc::new(Value::Str(c.to_string().into())))
                .collect()
        } else {
            s.split(&*sep)
                .map(|p| Rc::new(Value::Str(p.into())))
                .collect()
        };
        Ok(Rc::new(Value::Array(parts)))
    })
}

/// `(str-join arr sep)`: joins an array's elements (each rendered via
/// [`Value::display`]) with `sep` between them.
fn str_join() -> NativeFn {
    NativeFn::pure("str-join".into(), 2, |args| {
        let sep = arg_str("str-join", &args[1])?;
        match &*args[0] {
            Value::Array(xs) => {
                let parts: Vec<String> = xs.iter().map(|x| x.display()).collect();
                Ok(Rc::new(Value::Str(parts.join(sep.as_ref()).into())))
            }
            other => Err(RuntimeError::type_mismatch("str-join", "array", other)),
        }
    })
}

/// `(str-replace s from to)`: replaces all non-overlapping occurrences.
fn str_replace() -> NativeFn {
    NativeFn::pure("str-replace".into(), 3, |args| {
        let s = arg_str("str-replace", &args[0])?;
        let from = arg_str("str-replace", &args[1])?;
        let to = arg_str("str-replace", &args[2])?;
        Ok(Rc::new(Value::Str(s.replace(&*from, &to).into())))
    })
}

/// `(str->int s)`: parses a decimal integer, or `()` on failure. Surrounding
/// whitespace is ignored.
fn str_to_int() -> NativeFn {
    NativeFn::pure("str->int".into(), 1, |args| {
        let s = arg_str("str->int", &args[0])?;
        Ok(Rc::new(match s.trim().parse::<i64>() {
            Ok(n) => Value::Int(n),
            Err(_) => Value::Unit,
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RizzError;

    fn run(src: &str) -> Result<Rc<Value>, RizzError> {
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
            Err(RizzError::RuntimeError(RuntimeError::TypeMismatch { .. }))
        ));
    }

    #[test]
    fn split_and_join() {
        assert_eq!(*run_ok("(len (str-split \"a,b,c\" \",\"))"), Value::Int(3));
        assert_eq!(
            *run_ok("(get (str-split \"a,b,c\" \",\") 1)"),
            Value::Str("b".into())
        );
        assert_eq!(*run_ok("(len (str-split \"abc\" \"\"))"), Value::Int(3));
        assert_eq!(
            *run_ok("(str-join [\"a\" \"b\" \"c\"] \",\")"),
            Value::Str("a,b,c".into())
        );
        // join renders non-strings via to-str semantics
        assert_eq!(
            *run_ok("(str-join [1 2 3] \"-\")"),
            Value::Str("1-2-3".into())
        );
    }

    #[test]
    fn replace_and_parse_int() {
        assert_eq!(
            *run_ok("(str-replace \"a.b.c\" \".\" \"/\")"),
            Value::Str("a/b/c".into())
        );
        assert_eq!(*run_ok("(str->int \"42\")"), Value::Int(42));
        assert_eq!(*run_ok("(str->int \"  7 \")"), Value::Int(7));
        assert_eq!(*run_ok("(str->int \"nope\")"), Value::Unit);
    }
}
