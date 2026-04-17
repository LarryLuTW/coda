use anyhow::{Context, Result};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use tracing::{debug, info, warn};

/// `log stream` predicate. Matches only the Spotify process, and only log
/// events whose message contains `PlaybackQueueInvalidation` — the MediaRemote
/// marker that fires on actual item transitions (track → track, track → ad,
/// ad → track), not on progress ticks.
const PREDICATE: &str =
    "process == \"Spotify\" AND eventMessage CONTAINS \"PlaybackQueueInvalidation\"";

/// Wraps the `log stream` child so it's SIGTERM'd and reaped on drop.
pub struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let pid = Pid::from_raw(self.0.id() as i32);
        let _ = signal::kill(pid, Signal::SIGTERM);
        let _ = self.0.wait();
    }
}

/// Start tailing Spotify's MediaRemote transitions via macOS unified log.
///
/// Returns:
/// - A `Receiver<()>` that receives a unit value on every Spotify playback
///   queue change.
/// - A `ChildGuard` that must be kept alive — dropping it kills the subprocess.
pub fn start_tailing() -> Result<(mpsc::Receiver<()>, ChildGuard)> {
    let mut child = Command::new("/usr/bin/log")
        .args(["stream", "--style", "compact", "--predicate", PREDICATE])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn `log stream` (macOS only)")?;

    let stdout = child
        .stdout
        .take()
        .context("log stream child had no stdout")?;

    let (tx, rx) = mpsc::channel::<()>();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    // `log stream --style compact` can emit a multi-line
                    // message for a single event (the `<private>` continuation
                    // lines). Only the first line contains the marker, so
                    // filtering on it gives exactly one trigger per transition.
                    if l.contains("PlaybackQueueInvalidation:") {
                        debug!("playback transition detected");
                        if tx.send(()).is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    warn!("log stream read error: {e}");
                    break;
                }
            }
        }
        info!("log stream reader thread exiting");
    });

    info!("tailing unified log for Spotify playback transitions");
    Ok((rx, ChildGuard(child)))
}
