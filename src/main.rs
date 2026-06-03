use clap::Parser as _;
use std::{fs, io, path::PathBuf};

use rizz::{ParseError, Parser, RizzError};

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to file. Defaults to stdin
    #[arg(short = 'f', long = "file", global = true)]
    file: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, clap::Subcommand, Default)]
enum Commands {
    /// Parse and eval
    #[default]
    Eval,
    /// Print the parsed s-expression
    Parse {
        #[arg(short = 'p', long = "pretty")]
        pretty: bool,
    },
}

fn run(opts: Cli) -> Result<(), RizzError> {
    let sexp = match opts.file {
        Some(path) => {
            let f = fs::File::open(path).map_err(|e| ParseError::from_io_error(e, None))?;
            Parser::new(f).parse()?
        }
        None => Parser::new(io::stdin()).parse()?,
    };

    match opts.command.unwrap_or_default() {
        Commands::Parse { pretty } => {
            if pretty {
                println!("{sexp:#?}")
            } else {
                println!("{sexp:?}")
            }
        }
        Commands::Eval => {
            let (out, _) = rizz::eval_forms(sexp, &rizz::prelude::env())?;
            println!("{out}");
        }
    }

    Ok(())
}

fn main() -> Result<(), RizzError> {
    run(Cli::parse())
}
