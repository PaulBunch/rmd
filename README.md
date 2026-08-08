# rmd

A lightweight, reliable reminder daemon and CLI for Linux.

Set one-shot reminders that survive reboots and show up as desktop notifications.

> **Philosophy:** `rmd` is not a task manager. It is a zero-friction, hard-interrupt system designed to protect your flow state—reserving push notifications strictly for urgent, time-bound events (like food on the stove or an immediate meeting). Read the full [Design Vision](docs/VISION.md).

## Features

- Simple CLI: `rmd +25m Tea is ready`
- Background daemon (auto-started by the CLI if needed)
- Persistent storage (`~/.local/state/rmd/reminders.json`)
- Missed reminders handling (grouped notification when the system was offline)

## Requirements

- Linux
- A D-Bus notification daemon (e.g., Dunst, Mako, SwayNC, or DE built-in)

## Installation

### Using Makefile (Recommended)

Builds the binary, installs it to `~/.local/bin/`, and sets up the systemd user service:

```bash
make install
```

To enable and start the daemon immediately:

```bash
systemctl --user enable --now rmd.service
```

To uninstall:

```bash
make uninstall
```

### Manual Installation

```bash
cargo build --release
install -Dm755 target/release/rmd ~/.local/bin/rmd
install -Dm644 extra/rmd.service ~/.config/systemd/user/rmd.service
systemctl --user daemon-reload
```

## Usage

```bash
# Set a reminder (relative or absolute time)
rmd +10m Check the oven
rmd +1h30m Call mom
rmd 18:30 Evening standup

# List active reminders
rmd ls

# Remove a reminder by ID
rmd rm 3
```

The daemon starts automatically when needed. You can also run it manually:

```bash
rmd daemon
```

## How it works

* CLI talks to a background daemon over a Unix domain socket
* Daemon keeps reminders in memory and on disk (atomic JSON writes)
* When a reminder is due, it sends a desktop notification via D-Bus
* On startup, the daemon processes any missed reminders

## Configuration & Paths

State is stored at:

```
$XDG_STATE_HOME/rmd/reminders.json
# (falls back to ~/.local/state/rmd/reminders.json)
```

Socket location:

```
$XDG_RUNTIME_DIR/rmd.sock
# (falls back to /tmp/rmd.sock)
```

## License

[MIT](LICENSE)
