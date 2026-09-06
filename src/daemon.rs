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

async fn send_notification(summary: &str, body: &str) {
    let config = load_config();
    if let Err(e) = send_notification_inner(&config.dbus_service, summary, body).await {
        eprintln!(
            "Failed to send notification via D-Bus service '{}': {}",
            config.dbus_service, e
        );
    }
}

async fn send_notification_inner(service: &str, summary: &str, body: &str) -> Result<()> {
    let connection = zbus::Connection::session().await?;
    let mut hints = std::collections::HashMap::new();
    hints.insert("urgency", zbus::zvariant::Value::from(2u8));

    connection
        .call_method(
            Some(service),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "rmd",
                0u32,
                "",
                summary,
                body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await?;
    Ok(())
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
            send_notification(&n.summary, &n.body).await;
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

                        send_notification("Reminder", &r.message).await;
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

    let (response, should_stop) = handle_request(req, reminders);

    if let Ok(data) = serde_json::to_string(&response) {
        let _ = writer.write_all(format!("{}\n", data).as_bytes()).await;
    }

    should_stop
}

fn handle_request(req: Request, reminders: &mut Vec<Reminder>) -> (Response, bool) {
    let mut should_stop = false;
    let response = match req {
        Request::Stop => {
            should_stop = true;
            Response::Ok("Stopping rmd daemon...".to_string())
        }
        Request::Add { time_spec, message } => {
            let config = load_config();
            match parse_time(&time_spec, &config.default_time) {
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
            }
        }
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

            // Sort based on filter requirements:
            // Triggered, Missed, and History tables must be sorted in reverse chronological order (newest first).
            // All other tables (Active, All/Log) remain chronological (oldest/earliest first).
            let is_reversed = matches!(
                filter,
                ListFilter::History | ListFilter::Missed | ListFilter::Triggered
            );

            if is_reversed {
                list.sort_by_key(|r| std::cmp::Reverse(r.trigger_at));
            } else {
                list.sort_by_key(|r| r.trigger_at);
            }

            let total = list.len();

            if let Some(n) = limit {
                if is_reversed {
                    // For reverse chronological lists, the most recent elements are at the beginning
                    list.truncate(n);
                } else if matches!(filter, ListFilter::All) {
                    // For All (chronological), the most recent elements are at the end
                    let start = list.len().saturating_sub(n);
                    list = list.split_off(start);
                } else {
                    // For Active (chronological), the nearest upcoming elements are at the beginning
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

    (response, should_stop)
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

    #[test]
    fn test_list_filter_sorting_and_limiting() {
        let now = chrono::Utc::now().timestamp();
        let reminders = vec![
            mock_reminder(ReminderId::Active(1), now + 10, Status::Active),
            mock_reminder(ReminderId::Active(2), now + 20, Status::Active),
            mock_reminder(ReminderId::History(1), now - 30, Status::Triggered),
            mock_reminder(ReminderId::History(2), now - 20, Status::Triggered),
            mock_reminder(ReminderId::History(3), now - 10, Status::Missed),
        ];

        // 1. List Active: should be sorted chronological (oldest/earliest first)
        let (response, _) = handle_request(
            Request::List {
                filter: ListFilter::Active,
                limit: None,
            },
            &mut reminders.clone(),
        );
        if let Response::List {
            reminders: list,
            total,
        } = response
        {
            assert_eq!(total, 2);
            assert_eq!(list[0].id, ReminderId::Active(1));
            assert_eq!(list[1].id, ReminderId::Active(2));
        } else {
            panic!("Expected Response::List");
        }

        // 2. List History: should be sorted in reverse chronological (newest first: Missed at now-10, Triggered at now-20, Triggered at now-30)
        let (response, _) = handle_request(
            Request::List {
                filter: ListFilter::History,
                limit: None,
            },
            &mut reminders.clone(),
        );
        if let Response::List {
            reminders: list,
            total,
        } = response
        {
            assert_eq!(total, 3);
            assert_eq!(list[0].id, ReminderId::History(3)); // trigger_at: now - 10
            assert_eq!(list[1].id, ReminderId::History(2)); // trigger_at: now - 20
            assert_eq!(list[2].id, ReminderId::History(1)); // trigger_at: now - 30
        } else {
            panic!("Expected Response::List");
        }

        // 3. List History with limit 2: should return the 2 most recent in reverse chronological order
        let (response, _) = handle_request(
            Request::List {
                filter: ListFilter::History,
                limit: Some(2),
            },
            &mut reminders.clone(),
        );
        if let Response::List {
            reminders: list,
            total,
        } = response
        {
            assert_eq!(total, 3);
            assert_eq!(list.len(), 2);
            assert_eq!(list[0].id, ReminderId::History(3)); // trigger_at: now - 10
            assert_eq!(list[1].id, ReminderId::History(2)); // trigger_at: now - 20
        } else {
            panic!("Expected Response::List");
        }

        // 4. List All/Log with limit 3: should be chronological, returning the 3 most recent reminders (now-10, now+10, now+20) in chronological order
        let (response, _) = handle_request(
            Request::List {
                filter: ListFilter::All,
                limit: Some(3),
            },
            &mut reminders.clone(),
        );
        if let Response::List {
            reminders: list,
            total,
        } = response
        {
            assert_eq!(total, 5);
            assert_eq!(list.len(), 3);
            assert_eq!(list[0].id, ReminderId::History(3)); // trigger_at: now - 10
            assert_eq!(list[1].id, ReminderId::Active(1)); // trigger_at: now + 10
            assert_eq!(list[2].id, ReminderId::Active(2)); // trigger_at: now + 20
        } else {
            panic!("Expected Response::List");
        }
    }
}
