use std::io::stdin;

use rizz::RizzError;

fn main() -> Result<(), RizzError> {
    let res = rizz::parse_and_run(stdin())?;
    println!("{}", res.0);
    Ok(())
}
