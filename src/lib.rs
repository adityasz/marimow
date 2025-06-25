use clap::builder::styling::{Style, AnsiColor};
use ctrlc;
use dirs;
use log::{self, debug, info, log_enabled};
use notify::{RecursiveMode, Watcher};
use regex::Regex;
use serde::Deserialize;
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const TAB: &str = "    ";

#[derive(Debug)]
pub enum ErrorKind {
    BadConfig(Box<str>, toml::de::Error),
    FileArgMissing,
    FileNotFound(Box<str>),
    NotAFile(Box<str>),
    Io(io::Error),
    Watch(notify::Error),
    MarimoExited(std::process::ExitStatus),
    MarimoFailedToStart
}

impl From<io::Error> for ErrorKind {
    fn from(e: io::Error) -> Self {
        ErrorKind::Io(e)
    }
}
impl From<notify::Error> for ErrorKind {
    fn from(e: notify::Error) -> Self {
        ErrorKind::Watch(e)
    }
}

#[derive(Deserialize)]
struct Config {
    cache_dir: String,
}

fn cache_dir() -> Result<PathBuf, ErrorKind> {
    let default_path = PathBuf::from(".marimow_cache");
    if let Some(config_path) =
        dirs::config_dir().and_then(|p| Some(p.join("marimow").join("config.toml")))
    {
        info!("Found config in {}", config_path.display());
        toml::from_str(&fs::read_to_string(&config_path)?)
            .map_err(|e| ErrorKind::BadConfig(config_path.to_string_lossy().into(), e))
            .and_then(|config: Config| Ok(PathBuf::from(config.cache_dir)))
    } else {
        Ok(default_path)
    }
}

fn convert_file(source_path: &Path, target_path: &Path) -> Result<(), ErrorKind> {
    let content = fs::read_to_string(source_path)?;

    let mut result = String::from("import marimo\n\napp = marimo.App()\n");
    let mut push_section = |header: &str, section: &str, contains_function: Option<&mut bool>| {
        Some(section)
            .map(|s| s.trim())
            .filter(|s| {
                s.lines()
                    .any(|line| !line.trim().is_empty() && !line.starts_with('#'))
            })
            .inspect(|_| {
                contains_function.map(|f| *f = true);
                result.push_str(header)
            })
            .map(|s| {
                s.lines().for_each(|line| {
                    (!line.is_empty()).then(|| result.push_str(TAB));
                    result.push_str(line);
                    result.push_str("\n");
                });
            });
    };
    let parts: Vec<&str> = Regex::new(r"(?m)^# %%").unwrap().split(&content).collect();
    parts
        .get(0)
        .map(|section| push_section("\nwith app.setup:\n", section, None));
    let mut contains_function = false;
    parts.iter().skip(1).for_each(|section| {
        push_section(
            "\n\n@app.cell\ndef _():\n",
            section,
            Some(&mut contains_function),
        );
    });
    contains_function.then(|| result.push_str("\n")); // two empty lines after functions
    result.push_str(&format!("\nif __name__ == \"__main__\":\n{TAB}app.run()\n"));

    Ok(fs::write(target_path, result)?)
}

fn check_file_exists(file: &Path) -> Result<(), ErrorKind> {
    let path_str = file.to_string_lossy();
    if !file.exists() {
        return Err(ErrorKind::FileNotFound(path_str.into()));
    }
    if !file.is_file() {
        return Err(ErrorKind::NotAFile(path_str.into()));
    }
    Ok(())
}

fn run_marimo(args: Vec<OsString>) -> Result<Child, ErrorKind> {
    if log_enabled!(log::Level::Info) {
        let mut message = String::from("Running `marimo edit --watch`");
        args.iter().for_each(|arg| {
            message.push_str(" ");
            message.push_str(&arg.to_string_lossy().into_owned());
        });
        info!("{}", message);
    }

    Command::new("marimo")
        .args(["edit", "--watch"])
        .args(args.iter().filter(|&arg| *arg != "--watch"))
        .spawn().or(Err(ErrorKind::MarimoFailedToStart))
}

fn watch_and_update_file(
    source_path: &Path,
    target_path: &Path,
    marimo_child: &mut Child,
) -> Result<(), ErrorKind> {
    info!("Watching source path: {}", source_path.display());
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    // watch parent directory because Remove(File) is one of the events emitted
    // when `:w` is executed in vim, causing everything to break
    watcher.watch(source_path.parent().unwrap(), RecursiveMode::NonRecursive)?;

    loop {
        if let Some(status) = marimo_child.try_wait()? {
            if status.success() {
                break;
            } else {
                return Err(ErrorKind::MarimoExited(status));
            }
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                debug!("Received event: {:?}", event);
                if event.paths.iter().any(|p| p.file_name().unwrap() == source_path)
                    && (event.kind.is_modify() || event.kind.is_create())
                {
                    info!(
                        "source file '{}' changed, converting...",
                        source_path.display()
                    );
                    run_convert_command(source_path, target_path)?;
                }
            }
            Ok(Err(e)) => return Err(ErrorKind::Watch(e)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                marimo_child
                    .kill()
                    .expect("could not kill the marimo process");
                panic!("Watcher disconnected")
            }
        }
    }

    Ok(())
}

pub fn run_convert_command(input: &Path, output: &Path) -> Result<(), ErrorKind> {
    check_file_exists(&input)?;
    output.parent().map_or(Ok(()), fs::create_dir_all)?;
    convert_file(&input, &output)?;
    Ok(())
}

pub fn run_edit_command(mut args: Vec<OsString>) -> Result<(), ErrorKind> {
    let cache_dir = cache_dir()?;
    info!("Using {} as the cache directory", cache_dir.display());

    let input_path: PathBuf;
    let cached_path: PathBuf;

    if let Some(arg) = args
        .iter_mut()
        .find(|arg| !arg.as_encoded_bytes().starts_with(b"-"))
    {
        input_path = PathBuf::from(std::mem::take(arg));
        cached_path = if cache_dir.is_absolute() {
            cache_dir.join(
                std::env::current_dir()?
                    .join(&input_path)
                    .strip_prefix("/")
                    .unwrap(),
            )
        } else {
            cache_dir.join(&input_path)
        };
        *arg = cached_path.clone().into_os_string();
    } else {
        return Err(ErrorKind::FileArgMissing);
    }
    info!("Using {} as the cached file", cached_path.display());

    cached_path.parent().map_or(Ok(()), fs::create_dir_all)?;
    run_convert_command(&input_path, &cached_path)?;

    ctrlc::set_handler(|| {}).expect("Error setting Ctrl-C handler");

    let mut marimo_child = run_marimo(args)?;
    watch_and_update_file(&input_path, &cached_path, &mut marimo_child)?;

    marimo_child.wait()?;
    Ok(std::fs::remove_file(&cached_path)?)
}

pub fn clear_cache() -> Result<(), ErrorKind> {
    let style = if std::io::stdout().is_terminal() {
        Style::new().fg_color(Some(AnsiColor::Cyan.into()))
    } else {
        Style::new()
    };
    let cache_dir = cache_dir()?;
    println!("Removing cache at {style}{}{style:#}", cache_dir.display());
    fs::remove_dir_all(&cache_dir)
        .or_else(|err| match err.kind() {
            io::ErrorKind::NotFound => Ok(()),
            _ => Err(err)
        })
        .map_err(Into::into)
}
