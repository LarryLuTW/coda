use anyhow::{Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use tracing::{debug, error, info};

/// Names of Spotify files that change on track transitions.
const WATCH_FILES: &[&str] = &[
    "recently_played.bnk",
    "recently_played.bnk.tmp",
    "ad-state-storage.bnk",
    "ad-state-storage.bnk.tmp",
];

/// Discover Spotify user directories under `~/Library/Application Support/Spotify/Users/`.
/// Returns paths like `.../Users/someuser-user/`.
fn discover_user_dirs() -> Result<Vec<PathBuf>> {
    let mut base = dirs::home_dir().context("could not determine home directory")?;
    base.push("Library/Application Support/Spotify/Users");

    if !base.exists() {
        anyhow::bail!(
            "Spotify user data directory not found: {}. Is Spotify installed?",
            base.display()
        );
    }

    let mut user_dirs = Vec::new();
    for entry in std::fs::read_dir(&base).context("failed to read Spotify Users directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with("-user") {
                    user_dirs.push(path);
                }
            }
        }
    }

    if user_dirs.is_empty() {
        anyhow::bail!("no Spotify user directories found in {}", base.display());
    }

    Ok(user_dirs)
}

/// Start watching Spotify data files for changes.
///
/// Returns:
/// - A `mpsc::Receiver<()>` that receives a unit value on each relevant file change.
/// - The `RecommendedWatcher` (must be kept alive for watching to continue).
pub fn start_watching() -> Result<(mpsc::Receiver<()>, RecommendedWatcher)> {
    let user_dirs = discover_user_dirs()?;

    // Collect all specific file paths we care about
    let mut watch_paths: Vec<PathBuf> = Vec::new();
    for dir in &user_dirs {
        for filename in WATCH_FILES {
            watch_paths.push(dir.join(filename));
        }
    }

    let (tx, rx) = mpsc::channel::<()>();

    // We watch the parent directories (the user dirs) since the specific
    // files may not exist yet. We filter events by path in the callback.
    let watch_paths_clone = watch_paths.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| match res {
            Ok(event) => {
                let dominated = event.paths.iter().any(|p| watch_paths_clone.contains(p));
                if dominated {
                    debug!("file event: {:?}", event.kind);
                    // Non-blocking send — if the channel is full, we don't block the watcher.
                    let _ = tx.send(());
                }
            }
            Err(e) => {
                error!("file watch error: {}", e);
            }
        },
        notify::Config::default(),
    )
    .context("failed to create file watcher")?;

    for dir in &user_dirs {
        info!("watching: {}", dir.display());
        watcher
            .watch(dir, RecursiveMode::NonRecursive)
            .with_context(|| format!("failed to watch directory: {}", dir.display()))?;
    }

    info!(
        "monitoring {} Spotify files across {} user(s)",
        watch_paths.len(),
        user_dirs.len()
    );

    Ok((rx, watcher))
}
