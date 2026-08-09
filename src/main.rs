mod time;
mod ui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use time::parse_time;

// =========================================================================
// DATA MODEL & CONFIG
// =========================================================================

/// Structure representing a single reminder
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Reminder {
    pub id: u64,
    pub message: String,
    pub trigger_at: i64, // Unix timestamp in seconds
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
    List,
    Remove { id: u64 },
    Stop,
}

/// Responses sent from the Daemon to the CLI
#[derive(Debug, Serialize, Deserialize)]
enum Response {
    Ok(String),
    Added(Reminder),
    Removed(Reminder),
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
    /// List active reminders
    Ls,
    /// Remove a reminder by ID
    Rm { id: u64 },
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
            Commands::Ls => send_request(Request::List, &config).await?,
            Commands::Rm { id } => send_request(Request::Remove { id }, &config).await?,
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
        // If rmd is invoked without subcommands or positional arguments, list active reminders
        send_request(Request::List, &config).await?;
    }

    Ok(())
}

// =========================================================================
// DAEMON (SERVER)
// =========================================================================

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

    // 1. Process missed notifications on daemon startup
    let now = chrono::Local::now().timestamp();
    let (missed, active): (Vec<Reminder>, Vec<Reminder>) =
        reminders.into_iter().partition(|r| r.trigger_at <= now);

    reminders = active;

    let config = load_config();
    if !missed.is_empty() {
        let notifications = ui::build_missed_notifications(&missed, &config.time_format);

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
        let now = chrono::Local::now().timestamp();
        reminders.sort_by_key(|r| r.trigger_at);

        // Calculate sleep duration until the earliest reminder
        let sleep_duration = if let Some(first) = reminders.first() {
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
            _ = tokio::time::sleep(sleep_duration), if !reminders.is_empty() => {
                let now = chrono::Local::now().timestamp();
                let mut remaining = Vec::new();

                for r in reminders {
                    if r.trigger_at <= now {
                        let _ = notify_rust::Notification::new()
                            .summary("Reminder")
                            .body(&r.message)
                            .urgency(notify_rust::Urgency::Critical)
                            .timeout(notify_rust::Timeout::Never)
                            .show();
                    } else {
                        remaining.push(r);
                    }
                }
                reminders = remaining;
                let _ = save_reminders(&reminders);
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
                };
                reminders.push(reminder.clone());
                if let Err(e) = save_reminders(&reminders) {
                    Response::Error(format!("Failed to save state: {}", e))
                } else {
                    Response::Added(reminder)
                }
            }
            Err(e) => Response::Error(format!("Invalid time format: {}", e)),
        },
        Request::List => Response::List(reminders.clone()),
        Request::Remove { id } => {
            if let Some(pos) = reminders.iter().position(|r| r.id == id) {
                let removed = reminders.remove(pos);
                if let Err(e) = save_reminders(&reminders) {
                    Response::Error(format!("Failed to save state: {}", e))
                } else {
                    Response::Removed(removed)
                }
            } else {
                Response::Error(format!("Reminder {} not found", id))
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

async fn send_request(req: Request, config: &Config) -> Result<()> {
    let socket_path = get_socket_path();

    // 1. Attempt to connect to the socket
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(_) => {
            if matches!(req, Request::Stop) {
                println!("ℹ Daemon is not running.");
                return Ok(());
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

    // 5. Output the response to the user
    match response {
        Response::Ok(msg) => println!("✓ {}", msg),
        Response::Added(reminder) => {
            println!(
                "✓ {}",
                ui::format_add_response(&reminder, &config.time_format)
            );
        }
        Response::Removed(reminder) => {
            println!(
                "✓ {}",
                ui::format_remove_response(&reminder, &config.time_format)
            );
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
