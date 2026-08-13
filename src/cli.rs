use crate::config::{Config, save_config};
use crate::ipc::{ListFilter, Request, Response, send_ipc, send_request};
use crate::time::parse_time;
use crate::types::{Reminder, ReminderId, TimeFormat};
use crate::ui;
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

#[derive(Parser, Debug)]
#[command(name = "rmd", version, about = "Lightweight persistent reminders")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Time specification and message as positional arguments
    #[arg(num_args = 1..)]
    pub raw_args: Vec<String>,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum TimeFormatChoice {
    Iso,
    Human,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Set display time format (iso, human)
    TimeFormat {
        #[arg(value_enum)]
        format: TimeFormatChoice,
    },
    /// Set default limit of active reminders shown by `rmd` / `rmd ls`
    Limit {
        /// Maximum number of active reminders to show
        limit: usize,
    },
    /// Reset all configuration settings to default values
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the background daemon
    Daemon,
    /// Manage application configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Show nearest active reminders
    Ls {
        /// Optional override for top N reminders
        n: Option<usize>,
    },
    /// Show active reminders
    #[command(alias = "act")]
    Active {
        /// Optional limit (top N nearest)
        n: Option<usize>,
    },
    /// Show processed reminders (Missed + Triggered)
    #[command(alias = "hist")]
    History {
        /// Optional limit (most recent N)
        n: Option<usize>,
    },
    /// Show only Missed reminders
    #[command(alias = "msd")]
    Missed {
        /// Optional limit (most recent N)
        n: Option<usize>,
    },
    /// Show only Triggered reminders
    #[command(alias = "trg")]
    Triggered {
        /// Optional limit (most recent N)
        n: Option<usize>,
    },
    /// Show all reminders in chronological order
    #[command(aliases = &["all", "everything"])]
    Log {
        /// Optional limit (most recent N)
        n: Option<usize>,
    },
    /// View detailed info for reminder(s)
    Info {
        /// Reminder IDs to inspect
        #[arg(required = true, num_args = 1..)]
        ids: Vec<ReminderId>,
    },
    /// Purge finished (triggered/missed) reminders from state
    Clean {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    /// Remove reminders by ID
    Rm {
        /// Reminder IDs to delete
        #[arg(required = true, num_args = 1..)]
        ids: Vec<ReminderId>,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    /// Stop the daemon
    Stop,
}

/// Universal helper for interactive user confirmations.
/// If `auto_yes` is true (e.g., -y/--yes flag passed), returns true immediately.
fn prompt_confirm(message: &str, auto_yes: bool) -> bool {
    if auto_yes {
        return true;
    }

    print!("{} [y/N]: ", message);
    let _ = io::stdout().flush(); // Flush stdout immediately since the prompt lacks a newline

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap_or_default();

    let trimmed = input.trim().to_lowercase();
    trimmed == "y" || trimmed == "yes"
}

/// Separates time specification and reminder message from raw positional arguments
pub fn parse_time_and_message(args: &[String]) -> Result<(String, String)> {
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

pub fn handle_config_command(cmd: ConfigCommands, config: &mut Config) -> Result<()> {
    match cmd {
        ConfigCommands::TimeFormat { format } => {
            config.time_format = match format {
                TimeFormatChoice::Human => TimeFormat::Human,
                TimeFormatChoice::Iso => TimeFormat::Iso,
            };
            save_config(config)?;
            println!("✓ Time format set to '{:?}'", format);
        }
        ConfigCommands::Limit { limit } => {
            config.limit = limit;
            save_config(config)?;
            println!("✓ Default display limit set to {}", limit);
        }
        ConfigCommands::Reset { yes } => {
            let msg = "Reset all configuration settings to defaults?";
            if !prompt_confirm(msg, yes) {
                println!("Canceled.");
                return Ok(());
            }

            *config = Config::default();
            save_config(config)?;
            println!("✓ Configuration reset to defaults.");
        }
    }
    Ok(())
}

/// Handles inspection of specific reminders by ID
async fn handle_info_command(ids: Vec<ReminderId>, config: &Config) -> Result<()> {
    let response = send_ipc(Request::List {
        filter: ListFilter::All,
        limit: None,
    })
    .await?;
    let existing_reminders = match response {
        Response::List { reminders, .. } => reminders,
        Response::Error(err) => {
            eprintln!("✗ Error: {}", err);
            return Ok(());
        }
        _ => Vec::new(),
    };

    let existing_map: HashMap<ReminderId, &Reminder> =
        existing_reminders.iter().map(|r| (r.id, r)).collect();

    let mut unique_ids = Vec::new();
    for id in ids {
        if !unique_ids.contains(&id) {
            unique_ids.push(id);
        }
    }

    let mut found_reminders = Vec::new();
    let mut missing_ids = Vec::new();

    for id in unique_ids {
        if let Some(&reminder) = existing_map.get(&id) {
            found_reminders.push(reminder);
        } else {
            missing_ids.push(id);
        }
    }

    if !missing_ids.is_empty() {
        let missing_str = missing_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("ℹ Warning: Reminder(s) [{}] not found", missing_str);
    }

    for reminder in found_reminders {
        ui::print_reminder_info(reminder, &config.time_format);
    }

    Ok(())
}

pub async fn handle_command(cmd: Commands, config: &mut Config) -> Result<()> {
    match cmd {
        Commands::Daemon => { /* handled in main */ }
        Commands::Config { command } => handle_config_command(command, config)?,
        Commands::Ls { n } => {
            send_request(
                Request::List {
                    filter: ListFilter::Active,
                    limit: Some(n.unwrap_or(config.limit)),
                },
                config,
            )
            .await?
        }
        Commands::Active { n } => {
            send_request(
                Request::List {
                    filter: ListFilter::Active,
                    limit: n,
                },
                config,
            )
            .await?
        }
        Commands::History { n } => {
            send_request(
                Request::List {
                    filter: ListFilter::History,
                    limit: n,
                },
                config,
            )
            .await?
        }
        Commands::Missed { n } => {
            send_request(
                Request::List {
                    filter: ListFilter::Missed,
                    limit: n,
                },
                config,
            )
            .await?
        }
        Commands::Triggered { n } => {
            send_request(
                Request::List {
                    filter: ListFilter::Triggered,
                    limit: n,
                },
                config,
            )
            .await?
        }
        Commands::Log { n } => {
            send_request(
                Request::List {
                    filter: ListFilter::All,
                    limit: n,
                },
                config,
            )
            .await?
        }
        Commands::Info { ids } => handle_info_command(ids, config).await?,
        Commands::Clean { yes } => {
            let msg = "Delete all triggered and missed reminders?";
            if !prompt_confirm(msg, yes) {
                println!("Canceled.");
                return Ok(());
            }

            send_request(Request::Clean, config).await?
        }
        Commands::Rm { ids, yes } => {
            let response = send_ipc(Request::List {
                filter: ListFilter::All,
                limit: None,
            })
            .await?;
            let existing_reminders = match response {
                Response::List { reminders, .. } => reminders,
                Response::Error(err) => {
                    eprintln!("✗ Error: {}", err);
                    return Ok(());
                }
                _ => Vec::new(),
            };

            let existing_ids: HashSet<ReminderId> =
                existing_reminders.iter().map(|r| r.id).collect();

            let (found_ids, missing_ids): (Vec<ReminderId>, Vec<ReminderId>) =
                ids.into_iter().partition(|id| existing_ids.contains(id));

            // Warn about non-existent IDs
            if !missing_ids.is_empty() {
                let missing_str = missing_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                eprintln!("ℹ Warning: Reminder(s) [{}] not found", missing_str);
            }

            // If there is nothing to delete, terminate without prompt
            if found_ids.is_empty() {
                return Ok(());
            }

            // Request confirmation ONLY for found IDs
            let ids_str = found_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let msg = format!("Delete reminder(s) [{}]?", ids_str);
            if !prompt_confirm(&msg, yes) {
                println!("Canceled.");
                return Ok(());
            }

            send_request(Request::Remove { ids: found_ids }, config).await?;
        }
        Commands::Stop => send_request(Request::Stop, config).await?,
    }
    Ok(())
}

pub async fn handle_raw_args(raw_args: &[String], config: &Config) -> Result<()> {
    // Check if raw_args consists solely of numbers (reminder IDs)
    if let Ok(ids) = raw_args
        .iter()
        .map(|s| s.parse::<ReminderId>())
        .collect::<Result<Vec<ReminderId>, _>>()
    {
        handle_info_command(ids, config).await?;
    } else {
        match parse_time_and_message(raw_args) {
            Ok((time_spec, message)) => {
                send_request(Request::Add { time_spec, message }, config).await?;
            }
            Err(e) => eprintln!("✗ Error: {}", e),
        }
    }
    Ok(())
}

// =========================================================================
// TESTS
// =========================================================================

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
