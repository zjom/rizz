use crate::{
    Env, RuntimeError,
    runtime::{NativeFn, Value},
};
use anyhow::anyhow;
use std::rc::Rc;

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
        let path = args[0]
            .as_str()
            .ok_or_else(|| RuntimeError::type_mismatch("open", "str", &args[0]))?;
        let f = std::fs::File::open(&*path)?;
        let (v, env2) = crate::parse_and_run(f).map_err(|e| anyhow!(e.to_string()))?;
        let env = env.clone().union(env2.filter(|(k, _)| !k.starts_with('_')));

        Ok((v, env))
    })
}
