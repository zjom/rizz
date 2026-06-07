use std::io::{self, Cursor};
use std::path::PathBuf;

use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{
    ColorMode as RlColorMode, Completer, EditMode as RlEditMode, Editor, Helper, Highlighter,
    Hinter,
};

use crate::{ParseError, Parser, RizzError, Runtime};

#[derive(Completer, Helper, Highlighter, Hinter)]
struct REPLHelper {}

impl Validator for REPLHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

        if input.is_empty() {
            return Ok(ValidationResult::Valid(None));
        }

        // If the input ends with a newline, it means the user pressed Enter on an empty line
        // at the end of a multi-line input, which we treat as a submission/abort.
        if input.ends_with('\n') {
            return Ok(ValidationResult::Valid(None));
        }

        match Parser::new(Cursor::new(input)).parse() {
            Ok(_) => Ok(ValidationResult::Valid(None)),
            Err(ParseError::ExpectedToken { .. }) => Ok(ValidationResult::Incomplete),

            Err(ParseError::IOError { source, .. })
                if matches!(source.kind(), io::ErrorKind::UnexpectedEof) =>
            {
                Ok(ValidationResult::Incomplete)
            }

            Err(e) => Ok(ValidationResult::Invalid(Some(e.to_string()))),
        }
    }
}

pub struct Repl {
    rl: Editor<REPLHelper, FileHistory>,
    cfg: ReplConfig,
    rt: Runtime,
}

impl Repl {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_config(ReplConfig::default(), Runtime::new())
    }
    pub fn with_config(cfg: ReplConfig, rt: Runtime) -> anyhow::Result<Self> {
        let rlcfg = rustyline::Config::builder()
            .edit_mode(cfg.edit_mode.into())
            .color_mode(cfg.color_mode.into())
            .build();
        let helper = REPLHelper {};
        let mut rl = Editor::with_config(rlcfg)?;
        rl.set_helper(Some(helper));

        match rl.load_history(&cfg.history_path) {
            Ok(()) => {}
            Err(ReadlineError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        Ok(Self { cfg, rl, rt })
    }
    pub fn run(&mut self) -> anyhow::Result<()> {
        println!("ff self — Ctrl-D to exit, blank line to submit/abort multi-line input");
        loop {
            let prompt = ">> ";
            let readline = self.rl.readline(prompt);
            match readline {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }

                    self.rl.add_history_entry(line.as_str()).ok();
                    self.handle_command(Command::from_str(&line));
                }
                Err(ReadlineError::Interrupted) => {
                    // Ctrl-C: Clear the current buffer and start over
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    break Ok(());
                }

                Err(e) => {
                    eprintln!("error: {}", e);
                    Err(e)?;
                }
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        match self.execute_command(command) {
            Ok(output) => {
                println!("{}", output);
                let _ = self.save_history();
            }
            Err(e) => eprintln!("{}", e),
        }
    }

    fn execute_command(&mut self, command: Command) -> Result<String, RizzError> {
        match command {
            Command::Repeat => {
                let line = self
                    .rl
                    .history()
                    .iter()
                    .rev()
                    .skip_while(|c| c.starts_with(";;"))
                    .take(1)
                    .next()
                    .cloned()
                    .unwrap_or(String::new());

                self.execute_command(Command::Program {
                    line: line.as_str(),
                })
            }

            Command::Program { line } => {
                let v = self.rt.eval(Cursor::new(line))?;
                Ok(v.to_string())
            }
        }
    }

    fn save_history(&mut self) -> anyhow::Result<()> {
        if self.cfg.should_write_history {
            self.rl.save_history(&self.cfg.history_path)?;
        }

        Ok(())
    }
}

enum Command<'a> {
    Repeat,
    Program { line: &'a str },
}

impl<'a> Command<'a> {
    fn from_str(s: &'a str) -> Self {
        let trimmed = s.trim_start();
        if trimmed.starts_with(";;repeat") || trimmed.starts_with(";;.") {
            return Command::Repeat;
        }

        Command::Program { line: s }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, bon::Builder, clap::Args)]
pub struct ReplConfig {
    #[arg(long, value_enum, default_value_t = EditMode::Vi)]
    #[builder(default = EditMode::Vi)]
    pub edit_mode: EditMode,

    #[arg(long, value_enum, default_value_t = ColorMode::Enabled)]
    #[builder(default = ColorMode::Enabled)]
    pub color_mode: ColorMode,

    #[arg(long, default_value = ".ff_history")]
    #[builder(default = ".ff_history", into)]
    pub history_path: PathBuf,

    #[arg(long, default_value_t = false)]
    #[builder(default = false)]
    pub should_write_history: bool,
}

impl Default for ReplConfig {
    fn default() -> Self {
        ReplConfig::builder().build()
    }
}

/// Owned version of rustyline::EditMode so we can derive clap::ValueEnum
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum EditMode {
    Emacs,
    Vi,
}

impl From<EditMode> for RlEditMode {
    fn from(value: EditMode) -> Self {
        match value {
            EditMode::Emacs => RlEditMode::Emacs,
            EditMode::Vi => RlEditMode::Vi,
        }
    }
}

/// Owned version of rustyline::ColorMode so we can derive clap::ValueEnum
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorMode {
    Enabled,
    Disabled,
    Force,
}

impl From<ColorMode> for RlColorMode {
    fn from(value: ColorMode) -> Self {
        match value {
            ColorMode::Enabled => RlColorMode::Enabled,
            ColorMode::Disabled => RlColorMode::Disabled,
            ColorMode::Force => RlColorMode::Forced,
        }
    }
}
