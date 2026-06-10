//! String builtins.
//!
//! Names share a `str-` prefix except for `to-str` (and its alias `str-of`),
//! which stringifies any value. Strings are immutable `Rc<str>` under the
//! hood, so every transform returns a new string.
//!
//! Length, indexing, slicing, and the higher-order transforms are
//! polymorphic and live in [`crate::prelude::collections`] — they treat a
//! string as a sequence of one-char strings.

use im::Vector;
use std::rc::Rc;

use crate::runtime::{Env, NativeFn, RuntimeError, Value};

/// All string builtins bound to their canonical names (plus the
/// `str-of` alias of `to-str`).
pub fn env() -> Env {
    let mut env = Env::of_builtins(vec![
        ("to-str", to_str()),
        ("str-upper", str_upper()),
        ("str-lower", str_lower()),
        ("str-trim", str_trim()),
        ("str-split", str_split()),
        ("str-join", str_join()),
        ("str-replace", str_replace()),
        ("str->int", str_to_int()),
    ]);

    let v = env
        .get(&Rc::<str>::from("to-str"))
        .expect("alias target")
        .clone();
    env = env.update("str-of".into(), v);
    env
}

/// `(to-str v)`: stringifies any value via [`Value::display`].
fn to_str() -> NativeFn {
    NativeFn::pure("to-str".into(), 1, |args| {
        Ok(Rc::new(Value::Str(args[0].display().into())))
    })
    .with_doc(
        "\
(to-str V)
(str-of V)

Returns str: V rendered as a string. Strs are returned unquoted;
all other values use their display form.

See also: (str->int S)."
            .into(),
    )
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
    .with_doc(
        "\
(str-upper S)

Returns an uppercased copy of S.

See also: (str-lower S)."
            .into(),
    )
}

/// `(str-lower s)`: lowercased copy.
fn str_lower() -> NativeFn {
    NativeFn::pure("str-lower".into(), 1, |args| {
        let s = arg_str("str-lower", &args[0])?;
        Ok(Rc::new(Value::Str(s.to_lowercase().into())))
    })
    .with_doc(
        "\
(str-lower S)

Returns a lowercased copy of S.

See also: (str-upper S)."
            .into(),
    )
}

/// `(str-trim s)`: leading/trailing whitespace removed.
fn str_trim() -> NativeFn {
    NativeFn::pure("str-trim".into(), 1, |args| {
        let s = arg_str("str-trim", &args[0])?;
        Ok(Rc::new(Value::Str(s.trim().into())))
    })
    .with_doc(
        "\
(str-trim S)

Returns a copy of S with leading and trailing whitespace removed."
            .into(),
    )
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
    .with_doc(
        "\
(str-split S SEP)

Splits S on SEP and returns an array of strs. An empty SEP splits
into individual characters.

See also: (str-join XS SEP)."
            .into(),
    )
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
            Value::Cons { .. } => {
                let parts: Vec<_> = Value::iter(&args[0]).map(|v| v.display()).collect();
                Ok(Rc::new(Value::Str(parts.join(sep.as_ref()).into())))
            }
            other => Err(RuntimeError::type_mismatch("str-join", "array", other)),
        }
    })
    .with_doc(
        "\
(str-join XS SEP)

Joins the elements of an array or cons list into one str with SEP
between each element. Non-str elements are stringified as by
(to-str V).

See also: (str-split S SEP), (to-str V)."
            .into(),
    )
}

/// `(str-replace s from to)`: replaces all non-overlapping occurrences.
fn str_replace() -> NativeFn {
    NativeFn::pure("str-replace".into(), 3, |args| {
        let s = arg_str("str-replace", &args[0])?;
        let from = arg_str("str-replace", &args[1])?;
        let to = arg_str("str-replace", &args[2])?;
        Ok(Rc::new(Value::Str(s.replace(&*from, &to).into())))
    })
    .with_doc(
        "\
(str-replace S FROM TO)

Returns a copy of S with every non-overlapping occurrence of FROM
replaced by TO."
            .into(),
    )
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
    .with_doc(
        "\
(str->int S)

Parses S as a decimal integer, ignoring surrounding whitespace.
Returns int on success, or () when S does not parse.

See also: (to-str V)."
            .into(),
    )
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
