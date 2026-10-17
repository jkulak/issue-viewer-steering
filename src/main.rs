//! `issue-viewer`: read the settings, listen on localhost, print the address.

use std::io::{ErrorKind, IsTerminal, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;

use issue_viewer::config::{self, ConfigError, Environment, Settings};
use issue_viewer::web;

const USAGE: &str = "\
Usage: issue-viewer [--env-file PATH] [--open]

Serves the issue files of the projects in PROJECTS on http://127.0.0.1:PORT and prints that
address. Settings come from the environment, then from a .env file in the current directory.

Options:
  --env-file PATH  Read the settings from PATH instead of ./.env (it must exist)
  --open           Open the address in the default browser once the viewer listens
  -h, --help       Print this help
  -V, --version    Print the version";

/// A configuration the viewer cannot start with: the person has to change something first.
const EXIT_CONFIG: u8 = 2;

struct Options {
    env_file: Option<PathBuf>,
    open: bool,
}

enum Command {
    Run(Options),
    Print(String),
}

fn parse_arguments(mut arguments: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut options = Options {
        env_file: None,
        open: false,
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(Command::Print(USAGE.to_string())),
            "-V" | "--version" => {
                return Ok(Command::Print(format!(
                    "issue-viewer {}",
                    env!("CARGO_PKG_VERSION")
                )));
            }
            "--open" => options.open = true,
            "--env-file" => match arguments.next() {
                Some(path) => options.env_file = Some(PathBuf::from(path)),
                None => return Err("--env-file needs a path".into()),
            },
            other => match other.strip_prefix("--env-file=") {
                Some(path) => options.env_file = Some(PathBuf::from(path)),
                None => return Err(format!("unknown argument {other:?}; see --help")),
            },
        }
    }
    Ok(Command::Run(options))
}

fn load(options: &Options) -> Result<Settings, ConfigError> {
    let environment = match &options.env_file {
        Some(path) => Environment::load(path, true)?,
        None => Environment::load(&PathBuf::from(".env"), false)?,
    };
    config::load_settings(&|key| environment.get(key))
}

fn describe(settings: &Settings) {
    for project in &settings.projects {
        let access = if project.read_only {
            "read-only"
        } else {
            "read-write"
        };
        tracing::info!(
            "project {}: {} ({access})",
            project.name,
            project.path.display()
        );
        if !project.path.is_dir() {
            tracing::warn!(
                "project {}: {} is not a directory",
                project.name,
                project.path.display()
            );
        }
    }
    match &settings.open_root {
        Some(root) => {
            let access = if root.read_only {
                "read-only"
            } else {
                "read-write"
            };
            tracing::info!("open root: {} ({access})", root.path.display());
            if !root.path.is_dir() {
                tracing::warn!("open root: {} is not a directory", root.path.display());
            }
        }
        None => tracing::info!("open root: off"),
    }
    for entry in &settings.github {
        match &entry.checkout {
            Some(checkout) => {
                tracing::info!(
                    "github {}: checkout {}",
                    entry.full_name(),
                    checkout.display()
                )
            }
            None => tracing::info!("github {}", entry.full_name()),
        }
    }
}

fn open_in_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(error) = std::process::Command::new(program).arg(url).spawn() {
        tracing::warn!("could not open {url} with {program}: {error}");
    }
}

async fn shutdown() {
    let interrupt = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("a handler for SIGTERM");
        tokio::select! {
            _ = interrupt => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = interrupt.await;
    tracing::info!("stopping");
}

fn main() -> ExitCode {
    let options = match parse_arguments(std::env::args().skip(1)) {
        Ok(Command::Run(options)) => options,
        Ok(Command::Print(text)) => {
            println!("{text}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("issue-viewer: {message}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let settings = match load(&options) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("Configuration error: {error}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    tracing_subscriber::fmt()
        .with_max_level(settings.log_level.as_tracing())
        .with_target(false)
        .with_ansi(std::io::stdout().is_terminal())
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a Tokio runtime");
    runtime.block_on(serve(settings, options.open))
}

async fn serve(settings: Settings, open: bool) -> ExitCode {
    let port = settings.port;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            eprintln!(
                "Port {port} on 127.0.0.1 is already in use: is the viewer already running? \
                 Stop it, or set PORT in .env to another port."
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("Cannot listen on {address}: {error}");
            return ExitCode::FAILURE;
        }
    };
    // With PORT=0 the system picks the port, so the address comes from the listener.
    let url = match listener.local_addr() {
        Ok(bound) => format!("http://{bound}"),
        Err(_) => format!("http://{address}"),
    };
    // The address is the first line on stdout, so a script can start the viewer and read it.
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{url}");
    let _ = stdout.flush();
    drop(stdout);
    describe(&settings);
    if open {
        open_in_browser(&url);
    }
    let app = web::App::new(settings);
    // Start reading GitHub now, so the first board does not open on an empty answer.
    app.github.poke();
    if let Err(error) = axum::serve(listener, web::router(app))
        .with_graceful_shutdown(shutdown())
        .await
    {
        eprintln!("The server stopped: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(arguments: &[&str]) -> Result<Command, String> {
        parse_arguments(arguments.iter().map(|argument| argument.to_string()))
    }

    #[test]
    fn no_arguments_read_the_dot_env_in_the_current_directory_if_it_is_there() {
        let Ok(Command::Run(options)) = parsed(&[]) else {
            panic!("expected to run")
        };
        assert!(options.env_file.is_none());
        assert!(!options.open);
    }

    #[test]
    fn an_env_file_can_be_named_with_a_space_or_an_equals_sign() {
        for arguments in [
            &["--env-file", "/x/.env", "--open"][..],
            &["--open", "--env-file=/x/.env"],
        ] {
            let Ok(Command::Run(options)) = parsed(arguments) else {
                panic!("expected to run")
            };
            assert_eq!(options.env_file, Some(PathBuf::from("/x/.env")));
            assert!(options.open);
        }
    }

    #[test]
    fn an_unknown_argument_or_a_missing_path_is_refused() {
        assert!(parsed(&["--port", "1"]).is_err());
        assert!(parsed(&["--env-file"]).is_err());
    }

    #[test]
    fn help_and_version_print_instead_of_running() {
        assert!(
            matches!(parsed(&["--help"]), Ok(Command::Print(text)) if text.starts_with("Usage:"))
        );
        assert!(
            matches!(parsed(&["-V"]), Ok(Command::Print(text)) if text.starts_with("issue-viewer "))
        );
    }
}
