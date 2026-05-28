pub mod eq;
pub mod numbers;

use crate::evaluator::Env;

pub fn env() -> Env {
    Env::new().union(numbers::env()).union(eq::env())
}

pub fn install(e: Env) -> Env {
    env().union(e)
}
