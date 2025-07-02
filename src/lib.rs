use clap::builder::styling::{AnsiColor, Style};
use ctrlc;
use dirs;
use log::{self, debug, error, info, log_enabled};
use nix::sys::{prctl, signal};
use notify::{RecursiveMode, Watcher};
use regex::Regex;
use serde::Deserialize;
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;
use std::time::Instant;

const TAB: &str = "    ";
const DEBOUNCE_DURATION: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub enum ErrorKind {
    BadConfig(Box<str>, toml::de::Error),
    FileArgMissing,
    FileNotFound(Box<str>),
    NotAFile(Box<str>),
    Io(Box<str>, io::Error),
    Watch(notify::Error),
    MarimoExited(std::process::ExitStatus),
    MarimoFailedToStart,
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
        toml::from_str(
            &fs::read_to_string(&config_path)
                .map_err(|e| ErrorKind::Io(config_path.to_string_lossy().into(), e))?,
        )
        .map_err(|e| ErrorKind::BadConfig(config_path.to_string_lossy().into(), e))
        .and_then(|config: Config| Ok(PathBuf::from(config.cache_dir)))
    } else {
        Ok(default_path)
    }
}

fn convert_file(source_path: &Path, target_path: &Path) -> Result<(), ErrorKind> {
    let content = fs::read_to_string(source_path)
        .map_err(|e| ErrorKind::Io(source_path.to_string_lossy().into(), e))?;

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

    Ok(fs::write(target_path, result)
        .map_err(|e| ErrorKind::Io(target_path.to_string_lossy().into(), e))?)
}

fn assert_file_exists(file: &Path) -> Result<(), ErrorKind> {
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

    let mut command = Command::new("marimo");
    command
        .args(["edit", "--watch"])
        .args(args.iter().filter(|&arg| *arg != "--watch"));

    unsafe {
        command.pre_exec(|| prctl::set_pdeathsig(signal::Signal::SIGKILL).map_err(|e| e.into()));
    }

    command.spawn().or(Err(ErrorKind::MarimoFailedToStart))
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
        if let Some(status) = marimo_child
            .try_wait()
            .map_err(|e| ErrorKind::Io("marimo".into(), e))?
        {
            if status.success() {
                break;
            } else {
                return Err(ErrorKind::MarimoExited(status));
            }
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                debug!("Received event: {:?}", event);
                if event.paths.iter().any(|p| p == source_path)
                    && (event.kind.is_modify() || event.kind.is_create())
                {
                    // because saving in vim results in a lot of events
                    // (and sometimes the file disappears when trying to read)
                    let mut last_event_time = Instant::now();
                    while last_event_time.elapsed() < DEBOUNCE_DURATION {
                        match rx.recv_timeout(DEBOUNCE_DURATION) {
                            Ok(Ok(_)) => {
                                last_event_time = Instant::now();
                                continue;
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                            Ok(Err(e)) => return Err(ErrorKind::Watch(e)),
                            Err(RecvTimeoutError::Disconnected) => {
                                marimo_child
                                    .kill()
                                    .expect("could not kill the marimo process");
                                marimo_child
                                    .wait()
                                    .expect("could not wait for marimo process to exit");
                                panic!("Watcher disconnected")
                            }
                        }
                    }
                    info!(
                        "source file '{}' changed, converting...",
                        source_path.display()
                    );
                    if let Err(e) = convert_file(source_path, target_path) {
                        error!("Error converting file");
                        marimo_child
                            .kill()
                            .expect("could not kill the marimo process");
                        marimo_child
                            .wait()
                            .expect("could not wait for marimo process to exit");
                        return Err(e);
                    }
                }
            }
            Ok(Err(e)) => return Err(ErrorKind::Watch(e)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                marimo_child
                    .kill()
                    .expect("could not kill the marimo process");
                marimo_child
                    .wait()
                    .expect("could not wait for marimo process to exit");
                panic!("Watcher disconnected")
            }
        }
    }

    Ok(())
}

fn make_parent(path: &Path) -> Result<(), ErrorKind> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ErrorKind::Io(parent.to_string_lossy().into(), e))?;
    }
    Ok(())
}

pub fn run_convert_command(input: &Path, output: &Path) -> Result<(), ErrorKind> {
    assert_file_exists(&input)?;
    make_parent(output)?;
    convert_file(&input, &output)?;
    Ok(())
}

pub fn run_edit_command(mut args: Vec<OsString>) -> Result<(), ErrorKind> {
    let cache_dir_rel = cache_dir()?;
    let cache_dir = cache_dir_rel
        .canonicalize()
        .map_err(|e| ErrorKind::Io(cache_dir_rel.to_string_lossy().into(), e))?;
    info!("Using {} as the cache directory", cache_dir.display());

    let input_path: PathBuf;
    let cached_path: PathBuf;

    if let Some(arg) = args
        .iter_mut()
        .find(|arg| !arg.as_encoded_bytes().starts_with(b"-"))
    {
        let given_path = PathBuf::from(std::mem::take(arg));
        match given_path.canonicalize() {
            Ok(canonical_path) => input_path = canonical_path,
            Err(e) => {
                assert_file_exists(&given_path)?;
                return Err(ErrorKind::Io(given_path.to_string_lossy().into(), e)); // should be unreachable
            }
        }
        if let Some(prefix) = cache_dir.parent()
            && input_path.starts_with(prefix)
        {
            cached_path = cache_dir.join(&input_path.strip_prefix(prefix).unwrap());
        } else {
            cached_path = cache_dir.join(&input_path.strip_prefix("/").unwrap());
        }
        *arg = cached_path.clone().into_os_string();
    } else {
        return Err(ErrorKind::FileArgMissing);
    }
    info!("Using {} as the cached file", cached_path.display());

    make_parent(&cached_path)?;
    convert_file(&input_path, &cached_path)?;

    ctrlc::set_handler(|| {}).expect("Error setting Ctrl-C handler");

    let mut marimo_child = run_marimo(args)?;
    watch_and_update_file(&input_path, &cached_path, &mut marimo_child)?;

    marimo_child
        .wait()
        .map_err(|err| ErrorKind::Io("marimo".into(), err))?;
    Ok(std::fs::remove_file(&cached_path)
        .map_err(|e| ErrorKind::Io(cached_path.to_string_lossy().into(), e))?)
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
        .or_else(|e| match e.kind() {
            io::ErrorKind::NotFound => Ok(()),
            _ => Err(e),
        })
        .map_err(|e| ErrorKind::Io(cache_dir.to_string_lossy().into(), e))
}
