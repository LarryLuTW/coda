use anyhow::{Context, Result};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

const OSASCRIPT: &str = "/usr/bin/osascript";
const SPOTIFY_PREFIX: &str = "tell application \"Spotify\" to ";
const BUNDLE_ID: &str = "com.spotify.client";

/// Run an AppleScript snippet and return its stdout (trimmed).
fn run_applescript(script: &str) -> Result<String> {
    let output = Command::new(OSASCRIPT)
        .args(["-e", script])
        .output()
        .context("failed to execute osascript")?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("osascript failed: {}", stderr.trim());
    }

    Ok(stdout)
}

/// Get the Spotify URL of the currently playing track.
pub fn get_current_track_url() -> Result<String> {
    run_applescript(&format!("{SPOTIFY_PREFIX}(get spotify url of current track)"))
}

/// Check whether the currently playing track is an ad.
pub fn is_ad_playing() -> Result<bool> {
    let url = get_current_track_url()?;
    Ok(url.starts_with("spotify:ad"))
}

/// Check whether the Spotify process is currently running.
pub fn is_running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", "Spotify"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Tell Spotify to quit via AppleScript.
pub fn quit() -> Result<()> {
    run_applescript(&format!("{SPOTIFY_PREFIX}quit"))?;
    Ok(())
}

/// Tell Spotify to play via AppleScript.
pub fn play() -> Result<()> {
    run_applescript(&format!("{SPOTIFY_PREFIX}play"))?;
    Ok(())
}

/// Get the current player state (e.g. "playing", "paused", "stopped").
pub fn get_player_state() -> Result<String> {
    run_applescript(&format!("{SPOTIFY_PREFIX}(get player state)"))
}

/// Launch Spotify. If `background` is true, it opens hidden in the background.
pub fn launch(background: bool) -> Result<()> {
    let mut cmd = Command::new("/usr/bin/open");
    if background {
        cmd.args(["--hide", "--background"]);
    }
    cmd.args(["-b", BUNDLE_ID]);

    let status = cmd.status().context("failed to launch Spotify")?;
    if !status.success() {
        anyhow::bail!("open command exited with {}", status);
    }
    Ok(())
}

/// Wait until the Spotify process has fully exited.
/// Polls `is_running()` every 200ms, up to `timeout`.
pub fn wait_for_quit(timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if !is_running() {
            debug!("Spotify has quit");
            return Ok(());
        }
        if start.elapsed() > timeout {
            warn!("timed out waiting for Spotify to quit");
            anyhow::bail!("Spotify did not quit within {:?}", timeout);
        }
        thread::sleep(Duration::from_millis(200));
    }
}

/// Wait until Spotify is launched and ready to accept AppleScript commands.
/// Polls `get_player_state()` every 500ms, up to `timeout`.
pub fn wait_for_ready(timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if get_player_state().is_ok() {
            debug!("Spotify is ready");
            return Ok(());
        }
        if start.elapsed() > timeout {
            warn!("timed out waiting for Spotify to become ready");
            anyhow::bail!("Spotify did not become ready within {:?}", timeout);
        }
        thread::sleep(Duration::from_millis(500));
    }
}
