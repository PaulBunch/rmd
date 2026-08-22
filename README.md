# rmd

A lightweight, reliable reminder daemon and CLI for Linux.

Set one-shot reminders that survive reboots and show up as desktop notifications.

> **Philosophy:** `rmd` is not a task manager. It is a zero-friction, hard-interrupt system designed to protect your flow state—reserving push notifications strictly for urgent, time-bound events (like food on the stove or an immediate meeting). Read the full [Design Vision](docs/VISION.md).

<p align="center">
  <img width="568" height="285" alt="rmd CLI usage example" src="https://github.com/user-attachments/assets/68337ed6-ad4f-4f71-be92-45aed907062d" />
</p>

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
# Relative duration (+ prefix optional, spaces allowed)
rmd 45s Push the tempo
rmd +10m Check the oven
rmd 1h 30m Call mom

# Relative days & weekdays (spaced or git-style @)
rmd today 18:30 Game over
rmd tomorrow 15:00 Join release call
rmd tomorrow@10:00 Standup meeting
rmd fri 22:00 Shut up and go to bed

# Absolute time & ISO / full dates
rmd 18:30 Evening standup
rmd 2026-8-10 9:00 Doctor appointment
rmd 2026-12-01@10:00 VPS domain renewal

# --- Inspection & Listing ---

# List nearest active reminders (top N based on config)
rmd
# or override limit:
rmd ls 10

# Show specific subsets (supports optional limit N)
rmd active [n]       # Active reminders (alias: act)
rmd history [n]      # Missed & Triggered (alias: hist)
rmd missed [n]       # Only Missed reminders (alias: msd)
rmd triggered [n]    # Only Triggered reminders (alias: trg)
rmd log [n]          # Full chronological list (aliases: all, everything)

# View detailed info for specific reminder(s)
rmd 3
rmd 1 2 5
# or: rmd info 1 2 5

# --- Management & Cleanup ---

# Purge finished and missed reminders from history (interactive confirmation)
rmd clean
rmd clean -y         # Skip confirmation prompt

# Remove specific reminders by ID
rmd rm 3
rmd rm 1 2 5 -y      # Skip confirmation prompt
```

> **Tip:** Quotes around date/time or message are optional. `rmd tomorrow 15:00 Call mom` and `rmd "tomorrow 15:00" "Call mom"` work identically.

## Daemon Management

The daemon runs in the background, maintaining timers and dispatching notifications.

* **Systemd Service (Recommended):** Enabling `rmd.service` ensures the daemon starts automatically on boot/login, so scheduled reminders fire even if you haven't opened a terminal.
* **CLI Auto-Start:** If the daemon is not running when you issue any `rmd` command, the CLI will automatically spawn it in the background as a fallback.

To manually run the daemon in the foreground:

```bash
rmd daemon
```

To stop the running daemon:

```bash
rmd stop
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

View current settings and active configuration file location:

```bash
rmd config show
```

Manage options via CLI:

```bash
rmd config time-format human   # Set display time format (human, iso)
rmd config limit 10            # Set default active reminders limit
rmd config reset               # Reset configuration to default values
```

### File Locations

Configuration file:

```text
$XDG_CONFIG_HOME/rmd/config.json
# (falls back to ~/.config/rmd/config.json)
```

State storage:

```text
$XDG_STATE_HOME/rmd/reminders.json
# (falls back to ~/.local/state/rmd/reminders.json)
```

Runtime socket:

```text
$XDG_RUNTIME_DIR/rmd.sock
# (falls back to /tmp/rmd.sock)
```

## License

[MIT](LICENSE)
