mod time;
mod ui;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use time::parse_time;

// =========================================================================
// DATA MODEL & CONFIG
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Missed,
    Triggered,
}

impl Default for Status {
    fn default() -> Self {
        Status::Active
    }
}

/// Structure representing a single reminder
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Reminder {
    pub id: u64,
    pub message: String,
    pub trigger_at: i64, // Unix timestamp in seconds

    #[serde(default)]
    pub status: Status,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TimeFormat {
    #[default]
    Iso, // "2026-08-09 13:45"
    Human, // " 1:59 Sun 9 Aug"
}

/// Application configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub time_format: TimeFormat,
}

// =========================================================================
// IPC PROTOCOL
// =========================================================================

/// Messages sent from the CLI to the Daemon
#[derive(Debug, Serialize, Deserialize)]
enum Request {
    Add { time_spec: String, message: String },
    List { all: bool },
    Clean,
    Remove { ids: Vec<u64> },
    Stop,
}

/// Responses sent from the Daemon to the CLI
#[derive(Debug, Serialize, Deserialize)]
enum Response {
    Ok(String),
    Added(Reminder),
    Removed(Vec<Reminder>),
    List(Vec<Reminder>),
    Error(String),
}

// =========================================================================
// CLI PARSER
// =========================================================================

#[derive(Parser, Debug)]
#[command(name = "rmd", version, about = "Lightweight persistent reminders")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Time specification and message as positional arguments
    #[arg(num_args = 1..)]
    raw_args: Vec<String>,

    /// Set time format and save to config (iso, human)
    #[arg(long, global = true)]
    set_time_format: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the background daemon
    Daemon,
    /// List reminders
    Ls {
        /// Show all reminders including past history (triggered and missed)
        #[arg(short, long)]
        all: bool,
    },
    /// View history of past and missed notifications
    History,
    /// Purge finished and missed reminders from state
    Clean,
    /// Remove reminders by ID
    Rm {
        /// Reminder IDs to delete
        #[arg(required = true, num_args = 1..)]
        ids: Vec<u64>,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    /// Stop the daemon
    Stop,
}

// =========================================================================
// PATHS & STORAGE HELPERS
// =========================================================================

/// State directory
fn get_state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("RMD_STATE_DIR") {
        PathBuf::from(path)
    } else {
        dirs::state_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .expect("Cannot find home dir")
                    .join(".local/state")
            })
            .join("rmd")
    }
}

/// Configuration directory
fn get_config_dir() -> PathBuf {
    if let Ok(path) = std::env::var("RMD_CONFIG_DIR") {
        PathBuf::from(path)
    } else {
        dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .expect("Cannot find home dir")
                    .join(".config")
            })
            .join("rmd")
    }
}

/// Get the socket path: $RMD_SOCKET_PATH -> /run/user/<UID>/rmd.sock -> /tmp/rmd.sock
fn get_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("RMD_SOCKET_PATH") {
        PathBuf::from(path)
    } else {
        dirs::runtime_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("rmd.sock")
    }
}

/// Get the state file path: $RMD_STATE_DIR/reminders.json or ~/.local/state/rmd/reminders.json
fn get_state_path() -> PathBuf {
    let dir = get_state_dir();
    std::fs::create_dir_all(&dir).ok();
    dir.join("reminders.json")
}

/// Get the config file path: $RMD_CONFIG_DIR/config.json or ~/.config/rmd/config.json
fn get_config_path() -> PathBuf {
    let dir = get_config_dir();
    std::fs::create_dir_all(&dir).ok();
    dir.join("config.json")
}

/// Load configuration from disk or return default
fn load_config() -> Config {
    let path = get_config_path();
    if !path.exists() {
        return Config::default();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

/// Save configuration to disk atomically
fn save_config(config: &Config) -> Result<()> {
    let path = get_config_path();
    let tmp_path = path.with_extension("json.tmp");

    let data = serde_json::to_string_pretty(config)?;
    std::fs::write(&tmp_path, data)?;

    let file = std::fs::File::open(&tmp_path)?;
    file.sync_all()?;
    std::fs::rename(tmp_path, path)?;

    Ok(())
}

/// Atomic JSON serialization and disk flush for reminders
fn save_reminders(reminders: &[Reminder]) -> Result<()> {
    let path = get_state_path();
    let tmp_path = path.with_extension("json.tmp");

    let data = serde_json::to_string_pretty(reminders)?;
    std::fs::write(&tmp_path, data)?;

    // Ensure data is flushed to disk and perform an atomic rename
    let file = std::fs::File::open(&tmp_path)?;
    file.sync_all()?;
    std::fs::rename(tmp_path, path)?;

    Ok(())
}

/// Load reminders from disk
fn load_reminders() -> Vec<Reminder> {
    let path = get_state_path();
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

// =========================================================================
// MAIN & ROUTING
// =========================================================================

/// Separates time specification and reminder message from raw positional arguments
fn parse_time_and_message(args: &[String]) -> Result<(String, String)> {
    // Iterate backwards from full length down to 1 token to find the longest valid time spec
    for i in (1..=args.len()).rev() {
        let candidate_time = args[..i].join(" ");
        if parse_time(&candidate_time).is_ok() {
            let message = args[i..].join(" ");
            if message.trim().is_empty() {
                anyhow::bail!("Reminder message cannot be empty");
            }
            return Ok((candidate_time, message));
        }
    }

    anyhow::bail!("Invalid time format in arguments: '{}'", args.join(" "))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = load_config();

    // If the flag is passed, update the config and save it immediately
    if let Some(fmt) = &cli.set_time_format {
        match fmt.as_str() {
            "human" => {
                config.time_format = TimeFormat::Human;
                save_config(&config)?;
                println!("✓ Time format set to 'human'");
            }
            "iso" => {
                config.time_format = TimeFormat::Iso;
                save_config(&config)?;
                println!("✓ Time format set to 'iso'");
            }
            _ => {
                eprintln!(
                    "✗ Error: Unknown format '{}'. Valid options: human, iso",
                    fmt
                );
            }
        }
    }

    // Route the logic depending on the arguments passed
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Daemon => run_daemon().await?,
            Commands::Ls { all } => send_request(Request::List { all }, &config).await?,
            Commands::History => send_request(Request::List { all: true }, &config).await?,
            Commands::Clean => send_request(Request::Clean, &config).await?,
            Commands::Rm { ids, yes } => {
                // Request the full list (all: true) to validate the ID from the history
                let response = send_ipc(Request::List { all: true }).await?;
                let existing_reminders = match response {
                    Response::List(list) => list,
                    Response::Error(err) => {
                        eprintln!("✗ Error: {}", err);
                        return Ok(());
                    }
                    _ => Vec::new(),
                };

                let existing_ids: std::collections::HashSet<u64> =
                    existing_reminders.iter().map(|r| r.id).collect();

                let (found_ids, missing_ids): (Vec<u64>, Vec<u64>) =
                    ids.into_iter().partition(|id| existing_ids.contains(id));

                // 2. Warn about non-existent IDs
                if !missing_ids.is_empty() {
                    if missing_ids.len() == 1 {
                        eprintln!("ℹ Warning: Reminder {} not found", missing_ids[0]);
                    } else {
                        let missing_str = missing_ids
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        eprintln!("ℹ Warning: Reminders [{}] not found", missing_str);
                    }
                }

                // If there is nothing to delete, terminate the work without questions.
                if found_ids.is_empty() {
                    return Ok(());
                }

                // 3. Request confirmation ONLY for found IDs
                if !yes {
                    let prompt_msg = if found_ids.len() == 1 {
                        format!("Delete reminder {}? [y/N]: ", found_ids[0])
                    } else {
                        let ids_str = found_ids
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("Delete reminders [{}]? [y/N]: ", ids_str)
                    };

                    print!("{}", prompt_msg);
                    std::io::stdout().flush()?;

                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;

                    let reply = input.trim().to_lowercase();
                    if reply != "y" && reply != "yes" {
                        println!("Canceled.");
                        return Ok(());
                    }
                }

                send_request(Request::Remove { ids: found_ids }, &config).await?;
            }
            Commands::Stop => send_request(Request::Stop, &config).await?,
        }
    } else if !cli.raw_args.is_empty() {
        match parse_time_and_message(&cli.raw_args) {
            Ok((time_spec, message)) => {
                send_request(Request::Add { time_spec, message }, &config).await?;
            }
            Err(e) => eprintln!("✗ Error: {}", e),
        }
    } else if cli.set_time_format.is_none() {
        // Calling `rmd` without arguments by default prints only active
        send_request(Request::List { all: false }, &config).await?;
    }

    Ok(())
}

// =========================================================================
// DAEMON (SERVER)
// =========================================================================

pub fn sync_on_startup(reminders: &mut [Reminder]) {
    let now = Utc::now().timestamp();

    for r in reminders.iter_mut() {
        if r.status == Status::Active && r.trigger_at <= now {
            r.status = Status::Missed;
        }
    }
}

async fn run_daemon() -> Result<()> {
    let socket_path = get_socket_path();

    if socket_path.exists() {
        // Checking if the previous copy of the demon is still alive
        if UnixStream::connect(&socket_path).await.is_ok() {
            println!("ℹ Daemon is already running.");
            return Ok(());
        }
        // The socket exists, but no one is responding (stale socket) - safely delete
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path).context("Failed to bind Unix Domain Socket")?;
    let mut reminders = load_reminders();

    // Guarantee sorting at startup:
    reminders.sort_by_key(|r| r.trigger_at);

    // 1. Process missed notifications on daemon startup
    let now = Utc::now().timestamp();
    let mut newly_missed = Vec::new();

    for r in reminders.iter_mut() {
        if r.status == Status::Active && r.trigger_at <= now {
            r.status = Status::Missed;
            newly_missed.push(r.clone());
        }
    }

    let config = load_config();
    if !newly_missed.is_empty() {
        let notifications = ui::build_missed_notifications(&newly_missed, &config.time_format);

        for n in notifications {
            let _ = notify_rust::Notification::new()
                .summary(&n.summary)
                .body(&n.body)
                .urgency(notify_rust::Urgency::Critical)
                .timeout(notify_rust::Timeout::Never)
                .show();
        }

        let _ = save_reminders(&reminders);
    }

    println!("rmd daemon started at {}", socket_path.display());

    // 2. Main Event Loop
    loop {
        let now = Utc::now().timestamp();

        // Find the earliest ACTIVE reminder
        let next_active = reminders
            .iter()
            .filter(|r| r.status == Status::Active)
            .min_by_key(|r| r.trigger_at);

        // Calculate sleep duration only until the earliest Active
        let sleep_duration = if let Some(first) = next_active {
            let diff = first.trigger_at - now;
            if diff > 0 {
                Duration::from_secs(diff as u64)
            } else {
                Duration::from_millis(10) // Trigger immediately
            }
        } else {
            Duration::from_secs(86400) // Sleep for a day if the list is empty
        };

        tokio::select! {
            // Branch 1: Timer tick
            _ = tokio::time::sleep(sleep_duration) => {
                let now = Utc::now().timestamp();
                let mut status_changed = false;

                for r in reminders.iter_mut() {
                    if r.status == Status::Active && r.trigger_at <= now {
                        r.status = Status::Triggered;
                        status_changed = true;

                        let _ = notify_rust::Notification::new()
                            .summary("Reminder")
                            .body(&r.message)
                            .urgency(notify_rust::Urgency::Critical)
                            .timeout(notify_rust::Timeout::Never)
                            .show();
                    }
                }

                if status_changed {
                    let _ = save_reminders(&reminders);
                }
            }

            // Branch 2: Incoming CLI command
            accept_res = listener.accept() => {
                if let Ok((stream, _)) = accept_res {
                    let should_stop = handle_ipc_client(stream, &mut reminders).await;
                    let _ = save_reminders(&reminders);
                    if should_stop {
                        break;
                    }
                }
            }
        }
    }

    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }
    println!("rmd daemon stopped");
    Ok(())
}

async fn handle_ipc_client(stream: UnixStream, reminders: &mut Vec<Reminder>) -> bool {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    if reader.read_line(&mut line).await.is_err() {
        return false;
    }

    let mut should_stop = false;

    let req: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::Error(format!("Invalid JSON: {}", e));
            if let Ok(data) = serde_json::to_string(&resp) {
                let _ = writer.write_all(format!("{}\n", data).as_bytes()).await;
            }
            return false;
        }
    };

    let response = match req {
        Request::Stop => {
            should_stop = true;
            Response::Ok("Stopping rmd daemon...".to_string())
        }
        Request::Add { time_spec, message } => match parse_time(&time_spec) {
            Ok(trigger_at) => {
                let id = reminders.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                let reminder = Reminder {
                    id,
                    trigger_at,
                    message,
                    status: Status::Active,
                };
                reminders.push(reminder.clone());

                // Sort by trigger time:
                reminders.sort_by_key(|r| r.trigger_at);

                if let Err(e) = save_reminders(&reminders) {
                    Response::Error(format!("Failed to save state: {}", e))
                } else {
                    Response::Added(reminder)
                }
            }
            Err(e) => Response::Error(format!("Invalid time format: {}", e)),
        },
        Request::List { all } => {
            let list = if all {
                reminders.clone()
            } else {
                reminders
                    .iter()
                    .filter(|r| r.status == Status::Active)
                    .cloned()
                    .collect()
            };
            Response::List(list)
        }
        Request::Clean => {
            let mut removed_count = 0;

            // Leave only active reminders
            reminders.retain(|r| {
                if r.status != Status::Active {
                    removed_count += 1;
                    false
                } else {
                    true
                }
            });

            if removed_count == 0 {
                Response::Ok("No history or missed reminders to clean.".to_string())
            } else if let Err(e) = save_reminders(&reminders) {
                Response::Error(format!("Failed to save state: {}", e))
            } else {
                Response::Ok(format!("Cleaned {} inactive reminder(s).", removed_count))
            }
        }
        Request::Remove { ids } => {
            let mut removed = Vec::new();

            // Leave only those whose IDs are NOT included in the list for deletion
            reminders.retain(|r| {
                if ids.contains(&r.id) {
                    removed.push(r.clone());
                    false
                } else {
                    true
                }
            });

            if removed.is_empty() {
                Response::Error("No matching reminders found".to_string())
            } else if let Err(e) = save_reminders(&reminders) {
                Response::Error(format!("Failed to save state: {}", e))
            } else {
                Response::Removed(removed)
            }
        }
    };

    if let Ok(data) = serde_json::to_string(&response) {
        let _ = writer.write_all(format!("{}\n", data).as_bytes()).await;
    }

    should_stop
}

// =========================================================================
// CLIENT (IPC)
// =========================================================================

async fn send_ipc(req: Request) -> Result<Response> {
    let socket_path = get_socket_path();

    // 1. Attempt to connect to the socket
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(_) => {
            if matches!(req, Request::Stop) {
                return Ok(Response::Ok("Daemon is not running.".to_string()));
            }

            // If the socket is unavailable, start the daemon in the background
            println!("ℹ Daemon is not running. Starting rmd daemon...");

            let exe = std::env::current_exe().context("Failed to get current executable path")?;

            Command::new(exe)
                .arg("daemon")
                .spawn()
                .context("Failed to auto-start daemon")?;

            // Give the daemon 200ms to initialize and create the socket
            tokio::time::sleep(Duration::from_millis(200)).await;

            // Retry connection
            UnixStream::connect(&socket_path)
                .await
                .context("Failed to connect to daemon after auto-start")?
        }
    };

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // 2. Serialize request to JSON + append a newline character (\n)
    let mut serialized = serde_json::to_string(&req)?;
    serialized.push('\n');

    // 3. Send request to the socket
    writer.write_all(serialized.as_bytes()).await?;

    // 4. Read the response
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: Response =
        serde_json::from_str(&line).context("Invalid response format from daemon")?;

    Ok(response)
}

async fn send_request(req: Request, config: &Config) -> Result<()> {
    let response = send_ipc(req).await?;

    // 5. Output the response to the user
    match response {
        Response::Ok(msg) => println!("✓ {}", msg),
        Response::Added(reminder) => {
            println!(
                "✓ {}",
                ui::format_add_response(&reminder, &config.time_format)
            );
        }
        Response::Removed(removed_list) => {
            for reminder in removed_list {
                println!(
                    "✓ {}",
                    ui::format_remove_response(&reminder, &config.time_format)
                );
            }
        }
        Response::List(reminders) => {
            ui::print_reminders_table(&reminders, &config.time_format);
        }
        Response::Error(err) => eprintln!("✗ Error: {}", err),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_vec(args: impl IntoIterator<Item = impl Into<String>>) -> Vec<String> {
        args.into_iter().map(Into::into).collect()
    }

    #[test]
    fn test_parse_simple_time_and_message() {
        let args = to_vec(["+5m", "Buy", "milk"]);
        let (time_spec, msg) = parse_time_and_message(&args).unwrap();
        assert_eq!(time_spec, "+5m");
        assert_eq!(msg, "Buy milk");
    }

    #[test]
    fn test_parse_spaced_keyword_date_and_time() {
        let args = to_vec(["tomorrow", "15:00", "Call", "mom"]);
        let (time_spec, msg) = parse_time_and_message(&args).unwrap();
        assert_eq!(time_spec, "tomorrow 15:00");
        assert_eq!(msg, "Call mom");
    }

    #[test]
    fn test_parse_multi_part_relative_time() {
        let args = to_vec(["2h", "30m", "Check", "the", "oven"]);
        let (time_spec, msg) = parse_time_and_message(&args).unwrap();
        assert_eq!(time_spec, "2h 30m");
        assert_eq!(msg, "Check the oven");
    }

    #[test]
    fn test_reject_empty_message() {
        let args = to_vec(["+10m"]);
        let res = parse_time_and_message(&args);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_reject_invalid_time() {
        let args = to_vec(["invalid", "time", "argument"]);
        let res = parse_time_and_message(&args);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Invalid time format"));
    }
}
