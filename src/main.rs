mod engine;
mod log_tail;
mod spotify;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// coda — restart Spotify when ads play
#[derive(Parser)]
#[command(name = "coda", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run in the foreground (Ctrl+C to stop)
    Run {
        /// Enable verbose (debug) logging
        #[arg(short, long)]
        verbose: bool,
    },

    /// Start as a background daemon
    Start {
        /// Enable verbose (debug) logging
        #[arg(short, long)]
        verbose: bool,
    },

    /// Stop the running daemon
    Stop,

    /// Show whether the daemon is running
    Status,
}

fn pid_file_path() -> PathBuf {
    PathBuf::from("/tmp/coda.pid")
}

fn log_file_path() -> Result<PathBuf> {
    let mut path = dirs::home_dir().context("cannot determine home directory")?;
    path.push("Library/Logs/coda.log");
    Ok(path)
}

/// Read PID from the PID file, returning None if it doesn't exist or is invalid.
fn read_pid() -> Option<i32> {
    let mut contents = String::new();
    fs::File::open(pid_file_path())
        .ok()?
        .read_to_string(&mut contents)
        .ok()?;
    contents.trim().parse().ok()
}

/// Check if a process with the given PID is alive.
fn is_process_alive(pid: i32) -> bool {
    // signal 0 doesn't send a signal but checks if the process exists
    signal::kill(Pid::from_raw(pid), None).is_ok()
}

/// Initialize tracing/logging. In foreground mode, log to stdout.
/// In daemon mode, log to a file.
fn init_logging(verbose: bool, to_file: bool) -> Result<()> {
    let filter = if verbose { "debug" } else { "info" };
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    if to_file {
        let log_path = log_file_path()?;
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("failed to open log file: {}", log_path.display()))?;

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(file)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    }

    Ok(())
}

/// Core run logic shared by foreground and daemon modes.
fn run_core() -> Result<()> {
    let (rx, _child) = log_tail::start_tailing()?;
    engine::run(rx)
}

fn cmd_run(verbose: bool) -> Result<()> {
    init_logging(verbose, false)?;
    info!("coda v{} — foreground mode", env!("CARGO_PKG_VERSION"));
    run_core()
}

fn cmd_start(verbose: bool) -> Result<()> {
    // Check if already running
    if let Some(pid) = read_pid() {
        if is_process_alive(pid) {
            anyhow::bail!("coda is already running (PID: {pid})");
        }
        // Stale PID file — clean it up
        let _ = fs::remove_file(pid_file_path());
    }

    // Fork to background
    use nix::unistd::{fork, setsid, ForkResult};
    match unsafe { fork() }.context("failed to fork")? {
        ForkResult::Parent { child } => {
            // Parent: write PID file and exit
            let mut f = fs::File::create(pid_file_path())
                .context("failed to write PID file")?;
            write!(f, "{}", child)?;
            println!("coda started (PID: {child})");
            println!("logs: {}", log_file_path()?.display());
            std::process::exit(0);
        }
        ForkResult::Child => {
            // Child: become session leader, run the daemon
            setsid().context("setsid failed")?;

            init_logging(verbose, true)?;
            info!("coda v{} — daemon started", env!("CARGO_PKG_VERSION"));

            run_core()
        }
    }
}

fn cmd_stop() -> Result<()> {
    match read_pid() {
        Some(pid) if is_process_alive(pid) => {
            // Signal the entire process group, not just the daemon. The
            // daemon becomes its own process group leader via `setsid()` in
            // `cmd_start`, so a negative PID here reaches both the daemon
            // and its `log stream` child — without this, the child is
            // orphaned on stop because Rust destructors don't run on
            // external SIGTERM.
            signal::kill(Pid::from_raw(-pid), Signal::SIGTERM)
                .context("failed to send SIGTERM")?;
            println!("coda stopped (PID: {pid})");
            let _ = fs::remove_file(pid_file_path());
            Ok(())
        }
        Some(_) => {
            // Stale PID file
            let _ = fs::remove_file(pid_file_path());
            println!("coda is not running (removed stale PID file)");
            Ok(())
        }
        None => {
            println!("coda is not running");
            Ok(())
        }
    }
}

fn cmd_status() -> Result<()> {
    match read_pid() {
        Some(pid) if is_process_alive(pid) => {
            println!("coda is running (PID: {pid})");
        }
        Some(_) => {
            let _ = fs::remove_file(pid_file_path());
            println!("coda is not running (cleaned up stale PID file)");
        }
        None => {
            println!("coda is not running");
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // No subcommand → print help
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
        Some(Commands::Run { verbose }) => cmd_run(verbose),
        Some(Commands::Start { verbose }) => cmd_start(verbose),
        Some(Commands::Stop) => cmd_stop(),
        Some(Commands::Status) => cmd_status(),
    }
}
