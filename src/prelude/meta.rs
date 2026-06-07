use crate::{
    Env, RuntimeError,
    consts::FILE_EXTENSION,
    runtime::{NativeFn, Value},
};
use anyhow::anyhow;
use std::{path::PathBuf, rc::Rc};

pub fn env() -> Env {
    Env::of_builtins(vec![("typeof", typeof_()), ("open", open())])
}

fn typeof_() -> NativeFn {
    NativeFn::pure("typeof".into(), 1, |args| {
        Ok(Rc::new(Value::Ident(Value::type_name(&args[0]).into())))
    })
}

fn open() -> NativeFn {
    NativeFn::impure("open".into(), 1, move |args, env| {
        let mut path = args[0]
            .as_str_or_ident()
            .ok_or_else(|| RuntimeError::type_mismatch("open", "str", &args[0]))
            .map(|s| PathBuf::from(s.as_ref()))?;
        if path.extension().is_none() {
            path.set_extension(FILE_EXTENSION);
        }
        let f = std::fs::File::open(path)?;
        let (v, env2) = crate::parse_and_run(f).map_err(|e| anyhow!(e.to_string()))?;
        let env = env.clone().union(env2.filter(|(k, _)| !k.starts_with('_')));

        Ok((v, env))
    })
}
