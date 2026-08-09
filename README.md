# rmd

A lightweight, reliable reminder daemon and CLI for Linux.

Set one-shot reminders that survive reboots and show up as desktop notifications.

> **Philosophy:** `rmd` is not a task manager. It is a zero-friction, hard-interrupt system designed to protect your flow state—reserving push notifications strictly for urgent, time-bound events (like food on the stove or an immediate meeting). Read the full [Design Vision](docs/VISION.md).

## Features

- Simple CLI: `rmd +25m Tea is ready`
- Background daemon (auto-started by CLI or managed via `systemd`)
- Persistent storage (`~/.local/state/rmd/reminders.json`)
- Missed reminders handling (grouped notification when the system was offline)

## Requirements

- Linux
- Rust toolchain (`cargo` 1.90+ to build from source)
- A D-Bus notification daemon (e.g., Fnott, Mako, Dunst, SwayNC, or DE built-in)

## Installation

### Using Makefile (Recommended)

Builds the release binary, installs it to `~/.local/bin/`, and sets up the systemd user unit:

```bash
make install
```

Enable and start the daemon service:

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
# Relative time (duration)
rmd +10m Check the oven
rmd +1h30m Call mom
rmd 45s Push the tempo

# Relative days & weekdays
rmd 'today 18:30' Game over
rmd 'fri 22:00' Shut up and go to bed

# Absolute time & dates
rmd 18:30 Evening standup
rmd '2026-08-10 09:00' Doctor appointment

# List active reminders
rmd
# or: rmd ls

# Remove a reminder by ID
rmd rm 3
```

## Daemon Management

The daemon runs in the background, maintaining timers and dispatching notifications.

* **Systemd Service (Recommended):** Enabling `rmd.service` ensures the daemon starts automatically on boot/login, so scheduled reminders fire even if you haven't opened a terminal.
* **CLI Auto-Start:** If the daemon is not running when you issue any `rmd` command, the CLI will automatically spawn it in the background as a fallback.

To manually run the daemon in the foreground:

```bash
rmd daemon
```

To view logs when running via systemd:

```bash
journalctl --user -u rmd.service -f
```

## How it works

* CLI communicates with the background daemon over a Unix domain socket
* Daemon retains reminders in memory and persists state to disk (atomic JSON writes)
* When a reminder triggers, a desktop notification is dispatched via D-Bus
* On startup, the daemon checks for and handles any missed reminders

## Configuration & Paths

State directory:

```
$XDG_STATE_HOME/rmd/reminders.json
# (falls back to ~/.local/state/rmd/reminders.json)
```

Runtime socket:

```
$XDG_RUNTIME_DIR/rmd.sock
# (falls back to /tmp/rmd.sock)
```

## License

[MIT](LICENSE)
