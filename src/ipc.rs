use crate::config::Config;
use crate::storage::get_socket_path;
use crate::types::{Reminder, ReminderId};
use crate::ui;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Messages sent from the CLI to the Daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Add { time_spec: String, message: String },
    List { all: bool },
    Clean,
    Remove { ids: Vec<ReminderId> },
    Stop,
}

/// Responses sent from the Daemon to the CLI
#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Ok(String),
    Added(Reminder),
    Removed(Vec<Reminder>),
    List(Vec<Reminder>),
    Error(String),
}

pub async fn send_ipc(req: Request) -> Result<Response> {
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

pub async fn send_request(req: Request, config: &Config) -> Result<()> {
    // Determine if the status column is needed before moving `req` to `send_ipc`
    let show_status = matches!(req, Request::List { all: true });
    let response = send_ipc(req).await?;

    // Output the response to the user
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
            ui::print_reminders_table(&reminders, &config.time_format, show_status);
        }
        Response::Error(err) => eprintln!("✗ Error: {}", err),
    }

    Ok(())
}
