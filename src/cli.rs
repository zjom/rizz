use crate::{Parser, Runtime};
use anyhow::bail;
use clap::Parser as _;
use std::io::IsTerminal;
use std::{fs, io, path::PathBuf};

use crate::repl::{Repl, ReplConfig};

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to file. When omitted, reads piped stdin (or starts the REPL
    /// with --interactive).
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

/// Where source bytes come from: an explicit `--file`, or piped stdin.
enum Input {
    File(PathBuf),
    Stdin,
}

pub fn run() -> anyhow::Result<()> {
    let opts = Cli::parse();
    let input = match opts.file {
        Some(path) => Input::File(path),
        None if opts.interactive && io::stdin().is_terminal() => {
            return Repl::new()?.run();
        }
        None if !io::stdin().is_terminal() => Input::Stdin,
        None => {
            bail!("no file specified and no content piped to stdin; pass -f FILE or -i for a REPL")
        }
    };

    match opts.command.unwrap_or_default() {
        Commands::Parse { pretty } => {
            let sexp = match &input {
                Input::File(path) => Parser::new(fs::File::open(path)?).parse()?,
                Input::Stdin => Parser::new(io::stdin().lock()).parse()?,
            };
            if pretty {
                println!("{sexp:#?}")
            } else {
                println!("{sexp:?}")
            }
        }
        Commands::Eval { repl: repl_cfg } => {
            let mut rt = Runtime::new();
            // RizzError holds `Rc`s and so isn't Send + Sync, which anyhow
            // requires; render it here — the CLI only displays it anyway.
            let result = match &input {
                Input::File(path) => rt.eval_file(path),
                Input::Stdin => rt.eval(io::stdin().lock()),
            };
            let out = result.map_err(|e| anyhow::anyhow!("{e}"))?;
            if opts.interactive {
                Repl::with_config(repl_cfg, rt)?.run()?;
            } else {
                println!("{out}");
            }
        }
    }

    Ok(())
}
