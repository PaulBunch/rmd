# Technical Specification

**Project Name:** `rmd`  
**Objective:** A lightweight, reliable CLI tool and background daemon for managing one-shot reminders on Linux.

## System Architecture

The project follows a client-server (IPC) architecture over **Unix Domain Sockets (UDS)**:

1. **Daemon (Server):** A background process running a Tokio async loop. It holds reminders in memory, sets timers, sends D-Bus desktop notifications, and listens on a UDS for incoming CLI commands.
2. **CLI (Client):** Parses user arguments, builds IPC requests, connects to the daemon's UDS, sends commands, awaits responses, and prints formatted output to the terminal. If the daemon is unreachable, the CLI automatically spawns it.
3. **Storage:** JSON persistence at `$XDG_STATE_HOME/rmd/reminders.json`. The file is loaded into memory on daemon startup. Every state mutation (add/remove/expire) triggers an atomic write pattern (`.tmp` file -> `fsync` -> atomic `rename`).

## Key Dependencies (Crates)

* **`tokio`** — Async runtime (`current_thread` flavor) for handling I/O, UDS IPC, and timer scheduling with minimal footprint.
* **`serde` / `serde_json`** — Serialization framework for JSON persistence and newline-delimited IPC protocol messages.
* **`clap`** — CLI argument parsing with declarative subcommand structures.
* **`chrono`** — Local time handling, relative/absolute parsing, and timestamp calculations.
* **`zbus`** — D-Bus client for sending native desktop notifications to configurable service destinations.

## Missed Reminders Recovery

On startup, the daemon filters stored reminders against the current Unix timestamp (`trigger_at <= now`):

* `count < 3`: Emits individual critical desktop notifications with a `[Missed at HH:MM]` tag.
* `count >= 3`: Emits a single grouped summary notification encouraging the user to run `rmd ls`.
* Triggered/missed items are purged from memory and the state file is atomically updated.

## IPC Protocol (Client <-> Server)

Communication takes place over `$XDG_RUNTIME_DIR/rmd.sock` (falling back to `/tmp/rmd.sock`).  
Messages are newline-delimited JSON lines (`serde_json`):

### Request
* `Add { time_spec: String, message: String }`
* `List`
* `Remove { id: u64 }`

### Response
* `Ok(String)`
* `List(Vec<Reminder>)`
* `Error(String)`

## Project Conventions

- **User Interface:** All CLI messages, desktop notifications, and error outputs are written in English.
- **Codebase:** Code, comments, git commit messages, and documentation are strictly maintained in English.
