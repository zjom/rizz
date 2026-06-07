use crate::{ParseError, Parser, Runtime};
use anyhow::{anyhow, bail};
use clap::Parser as _;
use std::io::IsTerminal;
use std::{fs, io, path::PathBuf};

use crate::repl::{Repl, ReplConfig};

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to file. Defaults to stdin
    #[arg(short = 'f', long = "file", global = true)]
    file: Option<PathBuf>,

    /// whether to run in interactive mode.
    ///   note: setting this flag with the `parse` command is a noop.
    #[arg(short = 'i', long, global = true)]
    interactive: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    /// Parse and eval
    Eval {
        #[command(flatten)]
        repl: ReplConfig,
    },
    /// Print the parsed s-expression
    Parse {
        #[arg(short = 'p', long = "pretty")]
        pretty: bool,
    },
}

impl Default for Commands {
    fn default() -> Self {
        Self::Eval {
            repl: ReplConfig::default(),
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let opts = Cli::parse();
    let Some(path) = opts.file else {
        if io::stdin().is_terminal() && opts.interactive {
            return Repl::new()?.run();
        } else {
            bail!(
                "no file specified and no content piped to stdin and interactive mode not enabled."
            );
        }
    };

    match opts.command.unwrap_or_default() {
        Commands::Parse { pretty } => {
            let f = fs::File::open(&path).map_err(|e| ParseError::from_io_error(e, None))?;
            let sexp = Parser::new(f).parse()?;
            if pretty {
                println!("{sexp:#?}")
            } else {
                println!("{sexp:?}")
            }
        }
        Commands::Eval { repl: repl_cfg } => {
            let mut rt = Runtime::new();
            let out = rt.eval_file(&path).map_err(|e| anyhow!(e.to_string()))?;
            if opts.interactive {
                Repl::with_config(repl_cfg, rt)?.run()?;
            } else {
                println!("{out}");
            }
        }
    }

    Ok(())
}
