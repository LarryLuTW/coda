use anyhow::Result;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::spotify;

/// Minimum interval between restarts to avoid cascading restarts.
const DEBOUNCE_SECS: u64 = 15;

/// Timeout waiting for Spotify to quit after issuing quit command.
const QUIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout waiting for Spotify to be ready after relaunch.
const READY_TIMEOUT: Duration = Duration::from_secs(15);

/// Run the main event loop. Receives file-change signals from the watcher
/// and restarts Spotify whenever an ad is detected.
pub fn run(rx: mpsc::Receiver<()>) -> Result<()> {
    let mut last_restart = Instant::now() - Duration::from_secs(DEBOUNCE_SECS + 1);

    info!("coda is running — listening for Spotify ad events");

    for () in rx {
        // Debounce: skip if we recently restarted
        if last_restart.elapsed() < Duration::from_secs(DEBOUNCE_SECS) {
            continue;
        }

        // Check if Spotify is actually running
        if !spotify::is_running() {
            continue;
        }

        // Query current track
        match spotify::is_ad_playing() {
            Ok(true) => {
                // Ad detected — restart Spotify
                warn!("ad detected, restarting Spotify...");
                if let Err(e) = restart_spotify() {
                    warn!("restart failed: {e:#}");
                } else {
                    last_restart = Instant::now();
                }
            }
            Ok(false) => {
                // Normal track — log it
                if let Ok(url) = spotify::get_current_track_url() {
                    info!("now playing: {url}");
                }
            }
            Err(e) => {
                // AppleScript failed — Spotify might be in a transitional state
                warn!("could not check track: {e:#}");
            }
        }
    }

    info!("watcher channel closed, shutting down");
    Ok(())
}

/// Quit Spotify, relaunch it in the background, and resume playback.
fn restart_spotify() -> Result<()> {
    // 1. Quit
    spotify::quit()?;
    spotify::wait_for_quit(QUIT_TIMEOUT)?;

    // 2. Relaunch
    info!("relaunching Spotify...");
    spotify::launch(true)?;
    spotify::wait_for_ready(READY_TIMEOUT)?;

    // 3. Resume playback
    info!("resuming playback...");
    spotify::play()?;

    // Brief settle, then retry if still paused
    std::thread::sleep(Duration::from_secs(2));
    if let Ok(state) = spotify::get_player_state() {
        if state == "paused" {
            info!("still paused, retrying play...");
            spotify::play()?;
        }
    }

    info!("playback resumed");
    Ok(())
}
