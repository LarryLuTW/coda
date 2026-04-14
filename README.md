# coda

Restart Spotify when ads play. A lightweight macOS CLI daemon that detects Spotify ads via file system events and automatically restarts Spotify to skip them.

## How it works

1. **Event-driven detection** — Monitors Spotify's internal data files (`recently_played.bnk`, `ad-state-storage.bnk`) using macOS FSEvents. Zero CPU when idle.
2. **Ad identification** — When a track change is detected, queries Spotify via AppleScript. If the track URL starts with `spotify:ad`, it's an ad.
3. **Restart & resume** — Quits Spotify, relaunches it in the background, and issues play commands to resume your music.

## Install

### Homebrew (recommended)

```bash
brew install LarryLuTW/tap/coda
```

### Download binary

Download the latest release from [GitHub Releases](https://github.com/LarryLuTW/coda/releases), extract, and place `coda` somewhere on your `PATH`:

```bash
tar xzf coda-*.tar.gz
sudo mv coda /usr/local/bin/
```

> **Note**: macOS may show a Gatekeeper warning for downloaded binaries. Run `xattr -d com.apple.quarantine /usr/local/bin/coda` to clear it.

### Build from source

```bash
cargo install --path .
```

## Usage

```
coda              # Show help
coda run          # Run in foreground (Ctrl+C to stop)
coda run -v       # Run with verbose logging
coda start        # Start as background daemon
coda stop         # Stop the daemon
coda status       # Check if daemon is running
```

### Foreground mode

```bash
coda run
```

Logs to stdout. Press `Ctrl+C` to stop.

### Daemon mode

```bash
coda start
# coda started (PID: 12345)
# logs: /Users/you/Library/Logs/coda.log

coda status
# coda is running (PID: 12345)

coda stop
# coda stopped (PID: 12345)
```

## Requirements

- macOS (uses FSEvents and AppleScript)
- Spotify desktop app installed
- Accessibility permissions for `osascript` (System Settings > Privacy & Security > Accessibility)

## License

MIT
