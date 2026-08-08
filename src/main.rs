mod ui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

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
}

/// Responses sent from the Daemon to the CLI
#[derive(Debug, Serialize, Deserialize)]
enum Response {
    Ok(String),
    List(Vec<Reminder>),
    Error(String),
}

// =========================================================================
// CLI PARSER
// =========================================================================

#[derive(Parser, Debug)]
#[command(
    name = "rmd",
    version = "0.1",
    about = "Lightweight persistent reminders"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Time specification (e.g. +5m, 14:30)
    time: Option<String>,

    /// Reminder message
    #[arg(num_args = 1..)]
    message: Option<Vec<String>>,

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
}

// =========================================================================
// PATHS & STORAGE HELPERS
// =========================================================================

/// Get the socket path: /run/user/<UID>/rmd.sock or /tmp/rmd.sock
fn get_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("rmd.sock")
}

/// Get the state file path: ~/.local/state/rmd/reminders.json
fn get_state_path() -> PathBuf {
    let mut path = dirs::state_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Cannot find home dir")
            .join(".local/state")
    });
    path.push("rmd");
    std::fs::create_dir_all(&path).ok();
    path.push("reminders.json");
    path
}

/// Get the config file path: ~/.config/rmd/config.json
fn get_config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Cannot find home dir")
            .join(".config")
    });
    path.push("rmd");
    std::fs::create_dir_all(&path).ok();
    path.push("config.json");
    path
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

/// Simple time parser (+5s, +10m, +2h, +1d or HH:MM)
fn parse_time(input: &str) -> Result<i64> {
    let now = chrono::Local::now().timestamp();

    if let Some(stripped) = input.strip_prefix('+') {
        let mut total_sec: i64 = 0;
        let mut num_str = String::new();

        for ch in stripped.chars() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
            } else {
                let val: i64 = num_str.parse().context("Invalid number in relative time")?;
                num_str.clear();
                match ch {
                    's' => total_sec += val,
                    'm' => total_sec += val * 60,
                    'h' => total_sec += val * 3600,
                    'd' => total_sec += val * 86400,
                    _ => anyhow::bail!("Unknown time unit: {}", ch),
                }
            }
        }
        if total_sec == 0 {
            anyhow::bail!("Invalid relative time specifier");
        }
        Ok(now + total_sec)
    } else if let Ok(time) = chrono::NaiveTime::parse_from_str(input, "%H:%M") {
        let today = chrono::Local::now().date_naive();
        let naive_dt = today.and_time(time);
        let local_dt = naive_dt
            .and_local_timezone(chrono::Local)
            .single()
            .context("Ambiguous local time")?;

        let mut target = local_dt.timestamp();
        if target <= now {
            // If specified time has passed today, schedule for tomorrow
            target += 86400;
        }
        Ok(target)
    } else {
        anyhow::bail!("Unsupported time format. Use relative (+5m, +1h) or HH:MM");
    }
}

// =========================================================================
// MAIN & ROUTING
// =========================================================================

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = load_config();

    // If the flag is passed, update the config and save it immediately
    if let Some(fmt) = &cli.set_time_format {
        config.time_format = match fmt.as_str() {
            "human" => TimeFormat::Human,
            "iso" => TimeFormat::Iso,
            _ => {
                eprintln!("Unknown format '{}'. Using current setting.", fmt);
                config.time_format.clone()
            }
        };
        save_config(&config)?;
    }

    // Route the logic depending on the arguments passed
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Daemon => run_daemon().await?,
            Commands::Ls => send_request(Request::List, &config).await?,
            Commands::Rm { id } => send_request(Request::Remove { id }, &config).await?,
        }
    } else if let (Some(time), Some(msg)) = (cli.time, cli.message) {
        // If no subcommand is provided but we have time and text (e.g. rmd +5m Hello)
        send_request(
            Request::Add {
                time_spec: time,
                message: msg.join(" "),
            },
            &config,
        )
        .await?;
    } else {
        // If no arguments were provided, print the help message
        use clap::CommandFactory;
        Cli::command().print_help()?;
    }

    Ok(())
}

// =========================================================================
// DAEMON (SERVER)
// =========================================================================

async fn run_daemon() -> Result<()> {
    let socket_path = get_socket_path();
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path).context("Failed to bind Unix Domain Socket")?;
    let mut reminders = load_reminders();

    // 1. Process missed notifications on daemon startup
    let now = chrono::Local::now().timestamp();
    let (missed, active): (Vec<Reminder>, Vec<Reminder>) =
        reminders.into_iter().partition(|r| r.trigger_at <= now);

    reminders = active;

    if !missed.is_empty() {
        if missed.len() < 3 {
            for m in &missed {
                let dt = chrono::DateTime::from_timestamp(m.trigger_at, 0)
                    .map(|t| t.format("%H:%M").to_string())
                    .unwrap_or_default();
                let _ = notify_rust::Notification::new()
                    .summary("rmd (Missed)")
                    .body(&format!("[Missed at {}] {}", dt, m.message))
                    .urgency(notify_rust::Urgency::Critical)
                    .timeout(notify_rust::Timeout::Never) // Keep notification visible until dismissed by user
                    .show();
            }
        } else {
            let _ = notify_rust::Notification::new()
                .summary("rmd")
                .body(&format!(
                    "{} missed notifications. Run 'rmd ls' for details.",
                    missed.len()
                ))
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
                    handle_ipc_client(stream, &mut reminders).await;
                    let _ = save_reminders(&reminders);
                }
            }
        }
    }
}

async fn handle_ipc_client(stream: UnixStream, reminders: &mut Vec<Reminder>) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    if reader.read_line(&mut line).await.is_err() {
        return;
    }

    let req: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::Error(format!("Invalid JSON: {}", e));
            if let Ok(data) = serde_json::to_string(&resp) {
                let _ = writer.write_all(format!("{}\n", data).as_bytes()).await;
            }
            return;
        }
    };

    let response = match req {
        Request::Add { time_spec, message } => match parse_time(&time_spec) {
            Ok(trigger_at) => {
                let next_id = reminders.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                let new_rem = Reminder {
                    id: next_id,
                    message,
                    trigger_at,
                };
                reminders.push(new_rem);

                let left_sec = trigger_at - chrono::Local::now().timestamp();
                let mins = left_sec / 60;
                let secs = left_sec % 60;

                Response::Ok(format!(
                    "Reminder #{} set (in {}m {}s)",
                    next_id, mins, secs
                ))
            }
            Err(e) => Response::Error(e.to_string()),
        },
        Request::List => Response::List(reminders.clone()),
        Request::Remove { id } => {
            let len_before = reminders.len();
            reminders.retain(|r| r.id != id);
            if reminders.len() < len_before {
                Response::Ok(format!("Reminder #{} removed", id))
            } else {
                Response::Error(format!("Reminder #{} not found", id))
            }
        }
    };

    if let Ok(data) = serde_json::to_string(&response) {
        let _ = writer.write_all(format!("{}\n", data).as_bytes()).await;
    }
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
        Response::List(reminders) => {
            ui::print_reminders_table(&reminders, &config.time_format);
        }
        Response::Error(err) => eprintln!("✗ Error: {}", err),
    }

    Ok(())
}
