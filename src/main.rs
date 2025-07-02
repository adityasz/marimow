use clap::builder::styling::{AnsiColor, Style};
use clap::{Parser, Subcommand};
use marimow::{self, ErrorKind};
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert and edit a Python file in marimo with live reload.
    Edit {
        /// Arguments to pass to `marimo edit` command, including the
        /// file to watch.
        ///
        /// Note that `--watch` is automatically added to the arguments.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Convert a Python file to marimo format and save to output file.
    Convert {
        /// Path to the Python source file.
        input: PathBuf,
        /// Path to the output marimo file.
        output: PathBuf,
    }
}

struct ErrorReporter {
    prefix_style: Style,
    emph_style: Style,
    usage_style: Style,
}

impl Default for ErrorReporter {
    fn default() -> Self {
        if std::io::stderr().is_terminal() {
            ErrorReporter {
                prefix_style: Style::new().fg_color(Some(AnsiColor::Red.into())).bold(),
                emph_style: Style::new().bold(),
                usage_style: Style::new().bold().underline(),
            }
        } else {
            ErrorReporter {
                prefix_style: Style::new(),
                emph_style: Style::new(),
                usage_style: Style::new(),
            }
        }
    }
}

impl ErrorReporter {
    fn report(&self, error: &ErrorKind) -> ! {
        let emph = self.emph_style;
        let message = match error {
            ErrorKind::FileNotFound(path) => format!("file {0}'{path}'{0:#} does not exist", emph),
            ErrorKind::NotAFile(path) => format!("{0}'{path}'{0:#} is not a file", emph),
            ErrorKind::Io(path, e) => format!("IO error for {0}'{path}'{0:#}: {e}", emph),
            ErrorKind::Watch(e) => format!("{0}watch error{0:#}: {e}", emph),
            ErrorKind::MarimoFailedToStart => format!("marimo failed to start"),
            ErrorKind::MarimoExited(status) => {
                format!(
                    "marimo edit command failed with status {0:#}{status}{0:#}",
                    emph
                )
            }
            ErrorKind::ConfigFileNotFile(path) => {
                format!("config file {0}'{path}'{0:#} is not a file", emph)
            }
            ErrorKind::BadConfig(path, e) => {
                format!(
                    "the config file at {0}'{path}'{0:#} has error(s):\n{e}",
                    emph
                )
            }
            ErrorKind::FileArgMissing => {
                format!(
                    "file not specified\n{1}Usage:{1:#} {0}marimow edit{0:#} [OPTIONS] <FILE>",
                    emph, self.usage_style
                )
            }
        };
        eprintln!("{0}error:{0:#} {message}", self.prefix_style);
        std::process::exit(1);
    }
}

fn main() {
    let args = Cli::parse();
    let error_reporter = ErrorReporter::default();
    env_logger::init();

    let config = match marimow::load_config() {
        Ok(config) => config,
        Err(e) => error_reporter.report(&e),
    };

    let result = match args.command {
        Commands::Edit { args: marimo_args } => marimow::run_edit_command(marimo_args, &config),
        Commands::Convert { input, output } => {
            marimow::run_convert_command(&input, &output, &config)
        }
    };

    result.inspect_err(|e| error_reporter.report(e)).ok();
}
