mod cli;
mod config;
mod daemon;
mod ipc;
mod storage;
mod time;
mod types;
mod ui;

use anyhow::Result;
use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // 1. Parse command-line arguments
    let cli_args = cli::Cli::parse();

    // 2. Load configuration from disk
    let mut app_config = config::load_config();

    // 3. Route the execution flow depending on the arguments passed
    if let Some(cmd) = cli_args.command {
        match cmd {
            cli::Commands::Daemon => {
                // Start the background daemon process
                daemon::run().await?;
            }
            // Delegate remaining subcommands to the CLI handler (dispatches IPC requests)
            _ => cli::handle_command(cmd, &mut app_config).await?,
        }
    } else if !cli_args.raw_args.is_empty() {
        // Handle positional arguments (adding a new reminder or inspecting IDs)
        cli::handle_raw_args(&cli_args.raw_args, &app_config).await?;
    } else {
        // Calling `rmd` without arguments prints active reminders by default
        ipc::send_request(ipc::Request::List { all: false }, &app_config).await?;
    }

    Ok(())
}
