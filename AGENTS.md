# AGENTS.md — rmd

## Project
Linux reminder CLI + background daemon.
Single binary (`rmd`), package name `rmd-cli`.
Edition 2024. Philosophy: one-shot hard interrupts, not a task manager.
See `docs/VISION.md` and `docs/SPECIFICATION.md`.

## Layout
- `src/main.rs`       — entry point
- `src/cli.rs`        — argument parsing (clap), subcommands
- `src/daemon.rs`     — Tokio async loop, timers, notifications
- `src/ipc.rs`        — Unix domain socket protocol (newline-delimited JSON)
- `src/storage.rs`    — atomic JSON persistence
- `src/time.rs`       — relative/absolute/keyword datetime parsing
- `src/config.rs`     — XDG config
- `src/types.rs`      — shared types
- `src/ui.rs`         — terminal output formatting
- `extra/rmd.service` — systemd user unit

## Commands
- Check: `cargo check`
- Test: `cargo test`
- Clippy: `cargo clippy -- -D warnings`  (after non-trivial changes)
- Release build: `cargo build --release`
- Install (local): `make install`

## Tooling
- Search: always `rg`
- File discovery: `fd` if available, else `rg --files`

## Architecture constraints
- CLI ↔ daemon over Unix domain socket (`$XDG_RUNTIME_DIR/rmd.sock`)
- State: atomic JSON writes (tmp → fsync → rename) to `$XDG_STATE_HOME/rmd/reminders.json`
- Notifications: D-Bus via `notify-rust`
- Tokio runtime: keep footprint small (`current_thread` style where possible)
- Do not turn this into a task manager (no tags, priorities, recurring complex schedules)

## Rust rules
- No `.unwrap()` / `.expect()` outside tests — use `Result` + `?` (anyhow is already used)
- Avoid unnecessary `.clone()` / `.to_string()` when references work
- No `unsafe` unless explicitly requested
- Prefer small binary size (release profile already optimizes for size)

## Workflow
1. Read relevant modules with `rg` / file tools before editing
2. Make minimal, targeted changes
3. Run `cargo check` and `cargo test`
4. If broken — fix before ending the turn

## Language
Code, comments, commits, CLI messages, and notifications stay in English.
