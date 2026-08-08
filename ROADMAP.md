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
- [ ] Clarify the output formatting of the TIME column in the CLI table
- [ ] Refine CLI response formatting
- [ ] Refine the summary text and formatting of missed reminders
- [ ] Add daemon shutdown command (`rmd stop`)
- [ ] Advanced date & time parsing
  - [ ] Support full date-time specifiers (`YYYY-MM-DD HH:MM`, `tomorrow 15:00`, `mon 09:00`)
  - [ ] Validate targets against past timestamps and reject them with explicit error messages
- [ ] Reminder lifecycle & history management
  - [ ] Extend `Reminder` struct with an explicit status enum (`Active`, `Missed`, `Triggered`)
  - [ ] Retain triggered/missed reminders in state instead of instant purging
  - [ ] Implement `rmd clean` command to purge expired/read reminders
  - [ ] Add `rmd ls --all` or `rmd history` view for past notifications

## Phase 7: Release & Distribution

- [ ] Initial GitHub repository release
- [ ] Create AUR package for Arch Linux
- [ ] Submit "Show HN" post on Hacker News
