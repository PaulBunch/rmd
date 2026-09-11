# Roadmap

## Phase 1: Core Data Model & CLI Base

- [x] Initialize Cargo project (`rmd`)
- [x] Set up CLI parser with `clap` (`add`, `ls`, `rm`, `daemon`)
- [x] Define data models (`Reminder`, `Request`, `Response`)
- [x] Implement time parsing (convert relative `+5m` or absolute `14:30` to Unix timestamps)

## Phase 2: Persistence & Storage

- [x] Integrate `serde` and `serde_json`
- [x] Implement state loading from `$XDG_STATE_HOME/rmd/reminders.json`
- [x] Implement atomic state saving (`.tmp` write, `fsync`, atomic `rename`)

## Phase 3: IPC & Daemon Core

- [x] Integrate `tokio` async runtime
- [x] Set up Unix Domain Socket listener (`UnixListener`) at `$XDG_RUNTIME_DIR/rmd.sock`
- [x] Implement IPC request/response handling over UDS
- [x] Implement auto-spawning daemon mechanism in CLI client when socket is unreachable

## Phase 4: Timer Engine & Desktop Notifications

- [x] Build main async event loop (`tokio::select!`) balancing timers and IPC requests
- [x] Implement missed notifications recovery on daemon startup
- [x] Integrate `notify-rust` for desktop notifications via D-Bus
- [x] Automatically purge triggered reminders and flush state to disk

## Phase 5: System Integration

- [x] Create systemd user service unit (`rmd.service`)

## Phase 6: CLI Enhancements & Polish

- [x] Refine CLI table output formatting
- [x] Make the time display in the table in the OS time zone
- [x] Add `Left` time-remaining column with human-readable formatting (`2h 33m`, `1m 54s`, `3w 5d`, `2y 8mo` using `mo` for months to avoid `m`/`M` ambiguity)
- [x] Clarify the output formatting of the TIME column in the CLI table
- [x] Refine CLI response formatting
- [x] Display a list of reminders for `rmd` command without the flag instead of the current `help`
- [x] Refine the summary text and formatting of missed reminders
- [x] Add daemon shutdown command (`rmd stop`)
- [x] Advanced date & time parsing
  - [x] Support full date-time specifiers (`YYYY-MM-DD HH:MM`, `tomorrow 15:00`, `mon 09:00`)
  - [x] Validate targets against past timestamps and reject them with explicit error messages
  - [x] Add the ability to handle compound date-time values without quotes
- [x] Pull the application version into main.rs from Cargo.toml
- [x] Add multiple deletion of reminders by listing IDs separated by spaces
- [x] Add confirmation of reminder deletion
- [x] Reminder lifecycle & history management
  - [x] Extend `Reminder` struct with an explicit status enum (`Active`, `Missed`, `Triggered`)
  - [x] Retain triggered/missed reminders in state instead of instant purging
  - [x] Implement `rmd clean` command to purge expired/read reminders
  - [x] Add `rmd ls --all` or `rmd history` view for past notifications
  - [x] Add a status column to the history table
- [x] Add output of detailed info about reminder `rmd <ID>` to table (NAME VALUE)
- [x] Recycle IDs: assign lowest available integer to new reminders
- [x] Refactor codebase: split large modules to improve maintainability
  - [x] Refactor `main.rs`
  - [x] Refactor `ui.rs`
  - [x] Refactor `time.rs`
- [x] Refactor configuration flags into dedicated subcommands (keep only `-h` / `--help` and `-V` / `--version` as global options)
  - [x] Migrate `--set-time-format` to `rmd config time-format <iso|human>`.
  - [x] Add `rmd config limit <N>` — set default limit of active reminders shown by `rmd` / `rmd ls`
- [x] Implement consistent listing workflow:
  - `rmd` / `rmd ls [n]` — show nearest active reminders (default: top N from config; optional `[n]` overrides)
  - `rmd active [n]` (alias: `act`) — show active reminders (default: all; optional `[n]` = top N)
  - `rmd history [n]` (alias: `hist`) — show processed reminders (`Missed` + `Triggered`; default: all; optional `[n]` = most recent)
  - `rmd missed [n]` (alias: `msd`) — show only `Missed` reminders (default: all; optional `[n]` = most recent)
  - `rmd triggered [n]` (alias: `trg`) — show only `Triggered` reminders (default: all; optional `[n]` = most recent)
  - `rmd log [n]` (alias: `all`, `everything`) — show all reminders in chronological order (default: all; optional `[n]` = most recent)
- [x] UI & Table Rendering Improvements:
  - [x] Adjust `LEFT` column in history tables (`history`, `triggered`, `missed`): replace with `ELAPSED` or disable entirely instead of rendering static dashes (consider implementing flexible column management in `print_reminders_table`)
  - [x] Enable soft line wrapping for the `VALUE` column in the detailed view (`info`) instead of truncating text to terminal width
- [x] Add interactive confirmation prompt to `rmd clean` and refactor prompt logic into a unified helper shared with `rm` and `config reset`
- [x] Add `rmd config show` subcommand to display active settings loaded from `config.json`
- [x] Add configurable default time for date-only specifications:
  - [x] Introduce `default_time` setting in `config.json` (e.g., `"09:00"`) to replace fallback midnight (`00:00:00`) when time is omitted
  - [x] Add `rmd config default-time <HH:MM>` subcommand to customize the default trigger time
- [x] Extend datetime parser in `time.rs` to support:
  - [x] 12-hour format with AM/PM indicators (e.g., `02:00 PM`, `2pm`)
  - [x] Full month names and standard abbreviations (case-insensitive, e.g., `November`, `Nov`)
  - [x] Short aliases for common relative dates (e.g., `today` / `tod`, `tomorrow` / `tmr` / `tom`)
  - [x] Expand test coverage for new cases, including case-insensitivity checks

## Phase 7: Release & Distribution

- [x] Investigate Termux repository inclusion / packaging
- [x] Initial GitHub repository release
- [x] Submit "Show HN" post on Hacker News
- [x] Submit listing request to [Terminal Trove](https://terminaltrove.com)
- [x] Prepare a publication on [crates.io](https://crates.io) under the name `rmd-cli` (binary name stays `rmd`; crate name `rmd` is taken)
  - [x] Confirm the crate name is free (`cargo search rmd-cli`)
  - [x] Fill `[package]` metadata in `Cargo.toml`: `name = "rmd-cli"`, `description`, `license = "MIT"`, `repository`, `readme`, `keywords`, `categories`
  - [x] Add `[[bin]]` with `name = "rmd"` so `cargo install rmd-cli` drops `rmd` in `$PATH`
  - [x] Create a crates.io account (GitHub login + verified email) and `cargo login`
  - [x] Update `README.md`
- [x] Ship prebuilt binaries via GitHub Releases (musl, no Rust toolchain required)
  - [x] Add `.github/workflows/release.yml` (tag `v*` → `taiki-e/create-gh-release-action` + `taiki-e/upload-rust-binary-action`)
  - [x] Build `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
  - [x] Attach `.tar.gz` + `sha256` and include `extra/rmd.service` in the archive
  - [x] Name assets for humans (`rmd-linux-x86_64.tar.gz`, `rmd-linux-aarch64.tar.gz`)
  - [x] Make the prebuilt binary the primary install path in `README.md` (`make install` second)
  - [x] Push a `v*` tag and verify the binary runs on a machine without Rust
- [x] Publish to [crates.io](https://crates.io) as `rmd-cli` (`cargo publish --dry-run`, then `cargo publish`)
- [ ] Announce the binary release
  - [x] 2026-08-25: post on X (screenshot; repo URL in the first reply)
  - [x] Dev.to `#showdev` post (cover = CLI screenshot; embed `{% github PaulBunch/rmd %}`)
  - [x] Terminal Trove: submit via web form (image required); email = follow-up only
  - [x] Habr: adapt Dev.to `#showdev` post (Sandbox if needed)
  - [x] Share rmd on r/rust (This Week in Rust (TWiR) editors pick Project/Tooling Updates from there)
  - [ ] Optional: nominate rmd-cli for Crate of the Week
  - [ ] Optional: PR the DEV.to article into TWiR community section (not Project/Tooling)
- [ ] Create AUR package for Arch Linux (AUR new-account registration closed as of 2026-08; revisit when open)
- [ ] 2026-11-08: Submit [awesome-cli-apps](https://github.com/agarrharr/awesome-cli-apps)

## Phase 8: Documentation & System Man Page

- [x] Create `docs/TIME.md` as the single source of truth for date/time grammar, edge cases, and configuration defaults
- [ ] Write `scdoc` man page source (`extra/rmd.1.scd`) covering synopsis, commands, environment variables, and date/time syntax
- [ ] Update `Makefile` with `doc` generation target and install `rmd.1` to `$(PREFIX)/share/man/man1/`
- [ ] Include pre-compiled `extra/rmd.1` in release `.tar.gz` archives (`release.yml`)
- [ ] Refine time parser error output to point users to `--help` or `man rmd`

## Phase 9: Termux / Android Support (Under Evaluation)

- [ ] Evaluate feasibility and battery/lifecycle constraints of background daemon under Termux
  - [ ] Abstract notification backend logic to allow platform-specific implementations (`termux-notification` via `termux-api`)
  - [ ] Test daemon reliability without systemd (auto-spawning vs `termux-services` vs Android process killers)
  - [ ] Determine if target use-case is full daemon or CLI-only DB viewer/editor
- [ ] Write `build.sh` package recipe and submit PR to `termux/termux-packages` (if viable)

## Phase 10: Extensibility & Notification Targets

- [x] Support configurable D-Bus destination / service name
  - [x] Allow overriding target D-Bus service (default: `org.freedesktop.Notifications`)
  - [x] Keep core architecture pure by delegating further delivery (bridges, messengers, phone) to external D-Bus listeners
  - [x] Document limitations: Notifications are only delivered while the machine is awake. On suspend, the daemon is frozen and notifications are delayed until resume (appearing as missed).
- [ ] Design non-blocking event hooks architecture for custom scripts on reminder trigger (Under Evaluation)
  - [ ] Define scope and boundary: maintain core focus as a reminder manager (avoid overlap with `at` / `systemd-run`)
  - [ ] Determine execution semantics: async non-blocking execution, timeout limits, and environment context
  - [ ] Handle missed reminders edge-case (avoid script execution storms on daemon catch-up after system sleep)
  - [ ] Prototype optional `on_trigger_exec` or D-Bus event broadcasting

## Future / Optional Improvements

- [ ] Extend D-Bus target configuration to allow custom object path and interface
  - [ ] Currently hardcoded to path `/org/freedesktop/Notifications` and interface `org.freedesktop.Notifications` (compatible with 95%+ of desktop notification services and bridges).
  - [ ] Optionally allow specifying `dbus_path` and `dbus_interface` in `config.json` if non-standard custom D-Bus receivers require custom endpoints.
