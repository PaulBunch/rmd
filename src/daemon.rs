use crate::config::load_config;
use crate::ipc::{ListFilter, Request, Response};
use crate::storage::{get_socket_path, load_reminders, save_reminders};
use crate::time::parse_time;
use crate::types::{IdAllocator, Reminder, ReminderId, Status};
use crate::ui;
use anyhow::{Context, Result};
use chrono::Utc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

pub fn sync_on_startup(reminders: &mut [Reminder]) {
    let now = Utc::now().timestamp();
    let mut history_alloc = IdAllocator::new(reminders, true);

    for r in reminders.iter_mut() {
        if r.status == Status::Active && r.trigger_at <= now {
            r.status = Status::Missed;
            r.id = ReminderId::History(history_alloc.next_id()); // <- Go to history
        }
    }
}

pub async fn run() -> Result<()> {
    let socket_path = get_socket_path();

    if socket_path.exists() {
        // Checking if the previous copy of the daemon is still alive
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

    sync_on_startup(&mut reminders);

    let config = load_config();
    let newly_missed: Vec<_> = reminders
        .iter()
        .filter(|r| r.status == Status::Missed)
        .cloned()
        .collect();

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

    // Main Event Loop
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

                // Initialize the historical ID allocator before the loop
                let mut history_alloc = IdAllocator::new(&reminders, true);

                for r in reminders.iter_mut() {
                    if r.status == Status::Active && r.trigger_at <= now {
                        r.status = Status::Triggered;
                        r.id = ReminderId::History(history_alloc.next_id()); // <- Go to history
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
                let mut alloc = IdAllocator::new(reminders, false);
                let id = ReminderId::Active(alloc.next_id());
                let reminder = Reminder {
                    id,
                    trigger_at,
                    message,
                    status: Status::Active,
                };
                reminders.push(reminder.clone());

                // Sort by trigger time:
                reminders.sort_by_key(|r| r.trigger_at);

                if let Err(e) = save_reminders(reminders) {
                    Response::Error(format!("Failed to save state: {}", e))
                } else {
                    Response::Added(reminder)
                }
            }
            Err(e) => Response::Error(format!("Invalid time format: {}", e)),
        },
        Request::List { filter, limit } => {
            let mut list: Vec<Reminder> = reminders
                .iter()
                .filter(|r| match filter {
                    ListFilter::Active => r.status == Status::Active,
                    ListFilter::History => r.status != Status::Active,
                    ListFilter::Missed => r.status == Status::Missed,
                    ListFilter::Triggered => r.status == Status::Triggered,
                    ListFilter::All => true,
                })
                .cloned()
                .collect();

            // Always start with chronological sorting
            list.sort_by_key(|r| r.trigger_at);

            let total = list.len();

            if let Some(n) = limit {
                if matches!(
                    filter,
                    ListFilter::History
                        | ListFilter::Missed
                        | ListFilter::Triggered
                        | ListFilter::All
                ) {
                    let start = list.len().saturating_sub(n);
                    list = list.split_off(start);
                } else {
                    list.truncate(n);
                }
            }
            Response::List {
                reminders: list,
                total,
            }
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
            } else if let Err(e) = save_reminders(reminders) {
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
            } else if let Err(e) = save_reminders(reminders) {
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
// TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper for quick creation of mock reminders in tests
    fn mock_reminder(id: ReminderId, trigger_at: i64, status: Status) -> Reminder {
        Reminder {
            id,
            message: "Test".to_string(),
            trigger_at,
            status,
        }
    }

    // -------------------------------------------------------------------------
    // Integration test: Active -> History transition
    // -------------------------------------------------------------------------
    #[test]
    fn test_transition_active_to_history() {
        let now = chrono::Utc::now().timestamp();

        let mut reminders = vec![
            // Overdue active reminder (should be transitioned to history)
            mock_reminder(ReminderId::Active(1), now - 1000, Status::Active),
            // Up-to-date active reminder (should remain unchanged)
            mock_reminder(ReminderId::Active(2), now + 1000, Status::Active),
            // Existing history
            mock_reminder(ReminderId::History(1), now - 2000, Status::Triggered),
        ];

        // Invoke the lifecycle function (which should transition Active 1 into Missed/History 2)
        sync_on_startup(&mut reminders);

        // Check that the first reminder updated status and received the smallest h-ID (h2)
        assert_eq!(reminders[0].status, Status::Missed);
        assert_eq!(reminders[0].id, ReminderId::History(2));

        // Check that the second reminder was left untouched
        assert_eq!(reminders[1].status, Status::Active);
        assert_eq!(reminders[1].id, ReminderId::Active(2));

        // Check that old history was left untouched
        assert_eq!(reminders[2].status, Status::Triggered);
        assert_eq!(reminders[2].id, ReminderId::History(1));
    }
}
