use chrono::{Datelike, Local};
use terminal_size::{Width, terminal_size};

use crate::types::Reminder;
use crate::types::Status;
use crate::types::TimeFormat;

#[derive(Debug, PartialEq, Eq)]
pub struct NotificationPayload {
    pub summary: String,
    pub body: String,
}

// --- Helpers ---

/// Wraps text in ANSI escape sequences for underlining.
fn underline(text: &str) -> String {
    format!("\x1b[4m{}\x1b[0m", text)
}

/// Helper to softly wrap text by words to a specific width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        if paragraph.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current_line = String::new();
        for word in paragraph.split_whitespace() {
            if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_line.chars().count() + 1 + word.chars().count() <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn format_status_short(status: &Status) -> &'static str {
    match status {
        Status::Active => "Act",
        Status::Triggered => "Trg",
        Status::Missed => "Msd",
    }
}

pub fn format_status(status: &Status) -> &'static str {
    match status {
        Status::Active => "Active",
        Status::Triggered => "Triggered",
        Status::Missed => "Missed",
    }
}

/// Formats a timestamp for table output (preserves alignment padding).
pub fn format_datetime(timestamp: i64, time_format: &TimeFormat) -> String {
    let local_dt = chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.with_timezone(&Local))
        .unwrap_or_default();

    let current_year = Local::now().year();

    match time_format {
        TimeFormat::Iso => local_dt.format("%Y-%m-%d %H:%M").to_string(),
        TimeFormat::Human => {
            if local_dt.year() == current_year {
                // %k: hours with space ( 0..23)
                // %M: minutes with zero
                // %a: short day of the week (Sun)
                // %e: day with space ( 1..31)
                // %b: short month (Aug)
                local_dt.format("%k:%M %a %e %b").to_string()
            } else {
                local_dt.format("%k:%M %a %e %b %Y").to_string()
            }
        }
    }
}

/// Formats a timestamp for prose/inline CLI messages (collapses extra spacing).
pub fn format_datetime_prose(timestamp: i64, time_format: &TimeFormat) -> String {
    let raw = format_datetime(timestamp, time_format);
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn format_add_response(reminder: &Reminder, time_format: &TimeFormat) -> String {
    let time_str = format_datetime_prose(reminder.trigger_at, time_format);
    let now = Local::now().timestamp();
    let diff = reminder.trigger_at.saturating_sub(now);
    let left_str = format_time_left(if diff > 0 { diff as u64 } else { 0 });

    format!(
        "Added reminder {} in {}: {} — {}",
        reminder.id, left_str, time_str, reminder.message
    )
}

pub fn format_remove_response(reminder: &Reminder, time_format: &TimeFormat) -> String {
    let time_str = format_datetime_prose(reminder.trigger_at, time_format);
    format!(
        "Removed reminder {}: {} — {}",
        reminder.id, time_str, reminder.message
    )
}

/// Formats duration in seconds into a compact human-readable string.
/// Examples: "2y 8mo", "3w 5d", "2h 33m", "1m 54s", "42s"
fn format_time_left(secs: u64) -> String {
    if secs == 0 {
        return "0s".to_string();
    }

    const SEC_PER_MIN: u64 = 60;
    const SEC_PER_HOUR: u64 = 3600;
    const SEC_PER_DAY: u64 = 86400;
    const SEC_PER_WEEK: u64 = 604800; // 7 days
    const SEC_PER_MONTH: u64 = 2_592_000; // 30 days
    const SEC_PER_YEAR: u64 = 31_536_000; // 365 days

    if secs >= SEC_PER_YEAR {
        let years = secs / SEC_PER_YEAR;
        let rem = secs % SEC_PER_YEAR;
        let months = rem / SEC_PER_MONTH;
        if months > 0 {
            format!("{}y {}mo", years, months)
        } else {
            format!("{}y", years)
        }
    } else if secs >= SEC_PER_MONTH {
        let months = secs / SEC_PER_MONTH;
        let rem = secs % SEC_PER_MONTH;
        let weeks = rem / SEC_PER_WEEK;
        if weeks > 0 {
            format!("{}mo {}w", months, weeks)
        } else {
            format!("{}mo", months)
        }
    } else if secs >= SEC_PER_WEEK {
        let weeks = secs / SEC_PER_WEEK;
        let rem = secs % SEC_PER_WEEK;
        let days = rem / SEC_PER_DAY;
        if days > 0 {
            format!("{}w {}d", weeks, days)
        } else {
            format!("{}w", weeks)
        }
    } else if secs >= SEC_PER_DAY {
        let days = secs / SEC_PER_DAY;
        let rem = secs % SEC_PER_DAY;
        let hours = rem / SEC_PER_HOUR;
        if hours > 0 {
            format!("{}d {}h", days, hours)
        } else {
            format!("{}d", days)
        }
    } else if secs >= SEC_PER_HOUR {
        let hours = secs / SEC_PER_HOUR;
        let rem = secs % SEC_PER_HOUR;
        let mins = rem / SEC_PER_MIN;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}h", hours)
        }
    } else if secs >= SEC_PER_MIN {
        let mins = secs / SEC_PER_MIN;
        let rem = secs % SEC_PER_MIN;
        if rem > 0 {
            format!("{}m {}s", mins, rem)
        } else {
            format!("{}m", mins)
        }
    } else {
        format!("{}s", secs)
    }
}

// --- Table Rendering (Refactored) ---

const GAP: &str = "  "; // 2 spaces gap between columns

/// Represents a single prepared row for the terminal table.
struct TableRow {
    id: String,
    status: Option<String>,
    time: String,
    left: String,
    message: String,
}

/// Handles the sizing and layout calculation for the terminal table.
struct TableLayout {
    id_width: usize,
    st_width: usize,
    time_width: usize,
    left_width: usize,
    msg_width: usize,
    show_status: bool,
    left_col_name: String,
}

impl TableLayout {
    /// Constructs a layout by calculating the maximum required column widths.
    fn new(rows: &[TableRow], show_status: bool, left_col_name: String) -> Self {
        // Get terminal width (default to 80 if not running in a TTY)
        let term_width = terminal_size()
            .map(|(Width(w), _)| w as usize)
            .unwrap_or(80);

        let id_width = rows.iter().map(|r| r.id.len()).max().unwrap_or(0).max(2);

        let st_width = if show_status {
            rows.iter()
                .filter_map(|r| r.status.as_ref().map(|s| s.len()))
                .max()
                .unwrap_or(0)
                .max(3)
        } else {
            0
        };

        let time_width = rows.iter().map(|r| r.time.len()).max().unwrap_or(0).max(4);
        let left_width = rows
            .iter()
            .map(|r| r.left.len())
            .max()
            .unwrap_or(0)
            .max(left_col_name.len());

        let prefix_len = if show_status {
            id_width
                + GAP.len()
                + st_width
                + GAP.len()
                + time_width
                + GAP.len()
                + left_width
                + GAP.len()
        } else {
            id_width + GAP.len() + time_width + GAP.len() + left_width + GAP.len()
        };

        // Calculate maximum available space for the MESSAGE column to avoid line wraps
        let max_msg_avail = if term_width > prefix_len + 7 {
            term_width - prefix_len
        } else {
            7 // Minimum width for "MESSAGE"
        };

        let msg_width = rows
            .iter()
            .map(|r| r.message.chars().count())
            .max()
            .unwrap_or(0)
            .max(7)
            .min(max_msg_avail);

        Self {
            id_width,
            st_width,
            time_width,
            left_width,
            msg_width,
            show_status,
            left_col_name,
        }
    }

    /// Prints the table headers with correct padding and underline formatting.
    fn print_headers(&self) {
        let id_hdr = underline(&format!("{:<width$}", "ID", width = self.id_width));
        let time_hdr = underline(&format!("{:<width$}", "TIME", width = self.time_width));
        let left_hdr = underline(&format!(
            "{:<width$}",
            self.left_col_name,
            width = self.left_width
        ));
        let msg_hdr = underline(&format!("{:<width$}", "MESSAGE", width = self.msg_width));

        if self.show_status {
            let st_hdr = underline(&format!("{:<width$}", "STA", width = self.st_width));
            println!(
                "{}{GAP}{}{GAP}{}{GAP}{}{GAP}{}",
                id_hdr, st_hdr, time_hdr, left_hdr, msg_hdr
            );
        } else {
            println!(
                "{}{GAP}{}{GAP}{}{GAP}{}",
                id_hdr, time_hdr, left_hdr, msg_hdr
            );
        }
    }

    /// Prints a single row, truncating the message if it exceeds the calculated layout bounds.
    fn print_row(&self, row: &TableRow) {
        let truncated_msg = if row.message.chars().count() > self.msg_width {
            if self.msg_width > 3 {
                let mut s: String = row.message.chars().take(self.msg_width - 3).collect();
                s.push_str("...");
                s
            } else {
                row.message.chars().take(self.msg_width).collect()
            }
        } else {
            row.message.clone()
        };

        if self.show_status {
            let st_val = row.status.as_deref().unwrap_or("");
            println!(
                "{:<id_w$}{GAP}{:<st_w$}{GAP}{:<time_w$}{GAP}{:<left_w$}{GAP}{}",
                row.id,
                st_val,
                row.time,
                row.left,
                truncated_msg,
                id_w = self.id_width,
                st_w = self.st_width,
                time_w = self.time_width,
                left_w = self.left_width
            );
        } else {
            println!(
                "{:<id_w$}{GAP}{:<time_w$}{GAP}{:<left_w$}{GAP}{}",
                row.id,
                row.time,
                row.left,
                truncated_msg,
                id_w = self.id_width,
                time_w = self.time_width,
                left_w = self.left_width
            );
        }
    }
}

pub fn print_reminders_table(
    reminders: &[Reminder],
    total: usize,
    time_format: &TimeFormat,
    show_status: bool,
) {
    println!(); // Blank line before table

    if reminders.is_empty() {
        println!("No reminders found\n\n0 reminders");
        return;
    }

    let now_dt = Local::now();
    let now_ts = now_dt.timestamp();

    // Determine what name to give to the column:
    let has_active = reminders.iter().any(|r| r.status == Status::Active);
    let has_history = reminders.iter().any(|r| r.status != Status::Active);
    let left_col_name = match (has_active, has_history) {
        (true, false) => "LEFT",
        (false, true) => "ELAPSED",
        _ => "LEFT/ELAP", // If the table is mixed (for example, list all)
    }
    .to_string();

    let is_mixed = has_active && has_history;

    // 1. Prepare data rows (separation of concerns: map domain models to UI models)
    let rows: Vec<TableRow> = reminders
        .iter()
        .map(|r| {
            let status = show_status.then(|| format_status_short(&r.status).to_string());
            let time = format_datetime(r.trigger_at, time_format);

            let left = if r.trigger_at > now_ts {
                format_time_left((r.trigger_at - now_ts) as u64)
            } else if r.trigger_at < now_ts {
                let elapsed_str = format_time_left((now_ts - r.trigger_at) as u64);
                if is_mixed {
                    format!("- {}", elapsed_str)
                } else {
                    elapsed_str
                }
            } else {
                format_time_left(0) // In case of exact time matching, to avoid "-0s"
            };

            TableRow {
                id: r.id.to_string(),
                status,
                time,
                left,
                message: r.message.clone(),
            }
        })
        .collect();

    // 2. Calculate dynamic layout
    let layout = TableLayout::new(&rows, show_status, left_col_name);

    // 3. Render headers and rows
    layout.print_headers();
    for row in &rows {
        layout.print_row(row);
    }

    println!(); // Blank line after table

    // 4. Output summary
    let count = reminders.len();
    if count == total {
        if count == 1 {
            println!("1 reminder");
        } else {
            println!("{} reminders", count);
        }
    } else {
        println!("Showing {} of {} reminders", count, total);
    }
}

/// Prints key-value details for a single reminder formatted like a table.
pub fn print_reminder_info(reminder: &Reminder, time_format: &TimeFormat) {
    let now = Local::now().timestamp();

    let time_left_str = if reminder.trigger_at > now {
        let diff = (reminder.trigger_at - now) as u64;
        format_time_left(diff)
    } else {
        let diff = (now - reminder.trigger_at) as u64;
        format!("Elapsed: {}", format_time_left(diff))
    };

    let rows: Vec<(&'static str, String)> = vec![
        ("ID", reminder.id.to_string()),
        ("Status", format_status(&reminder.status).to_string()),
        (
            "Trigger At",
            format_datetime_prose(reminder.trigger_at, time_format),
        ),
        ("Time Left", time_left_str),
        ("Message", reminder.message.clone()),
    ];

    println!();

    let term_width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    let name_width = rows.iter().map(|(n, _)| n.len()).max().unwrap_or(0).max(4); // "NAME".len()

    let prefix_len = name_width + GAP.len();
    let max_val_avail = if term_width > prefix_len + 5 {
        term_width - prefix_len
    } else {
        5 // "VALUE".len()
    };

    let val_width = rows
        .iter()
        .map(|(_, v)| v.chars().count())
        .max()
        .unwrap_or(0)
        .max(5)
        .min(max_val_avail);

    let header_name = underline(&format!("{:<width$}", "NAME", width = name_width));
    let header_val = underline(&format!("{:<width$}", "VALUE", width = val_width));

    println!("{}{GAP}{}", header_name, header_val);

    for (name, val) in rows {
        // Split the line into several if it doesn't fit
        let wrapped_vals = wrap_text(&val, val_width);

        for (i, line) in wrapped_vals.iter().enumerate() {
            if i == 0 {
                // The first line is displayed with the key name
                println!("{:<name_w$}{GAP}{}", name, line, name_w = name_width);
            } else {
                // Sublines are aligned with an empty indent
                println!("{:<name_w$}{GAP}{}", "", line, name_w = name_width);
            }
        }
    }

    println!();
}

/// Formats notifications for missed reminders.
pub fn build_missed_notifications(
    missed: &[Reminder],
    fmt: &TimeFormat,
) -> Vec<NotificationPayload> {
    if missed.is_empty() {
        return vec![];
    }

    if missed.len() < 3 {
        missed
            .iter()
            .map(|m| {
                let dt = format_datetime_prose(m.trigger_at, fmt);
                NotificationPayload {
                    summary: "Missed Reminder".to_string(),
                    body: format!("{}\n{}", dt, m.message),
                }
            })
            .collect()
    } else {
        vec![NotificationPayload {
            summary: "Missed Reminders".to_string(),
            body: format!(
                "{} missed notifications.\nRun 'rmd ls' for details.",
                missed.len()
            ),
        }]
    }
}

// =========================================================================
// TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ReminderId;
    use crate::types::Status;
    use chrono::{Datelike, TimeZone};

    #[test]
    fn test_format_datetime_table_preserves_padding() {
        let current_year = Local::now().year();
        let dt = Local
            .with_ymd_and_hms(current_year, 8, 9, 4, 1, 0)
            .single()
            .unwrap();
        let ts = dt.timestamp();

        let table_formatted = format_datetime(ts, &TimeFormat::Human);

        // Should contain leading space for 4:01 and double space before 9
        assert_eq!(table_formatted, " 4:01 Sun  9 Aug");
    }

    #[test]
    fn test_format_datetime_prose_strips_padding() {
        let current_year = Local::now().year();
        let dt = Local
            .with_ymd_and_hms(current_year, 8, 9, 4, 1, 0)
            .single()
            .unwrap();
        let ts = dt.timestamp();

        let prose_formatted = format_datetime_prose(ts, &TimeFormat::Human);

        assert_eq!(prose_formatted, "4:01 Sun 9 Aug");
    }

    #[test]
    fn test_format_datetime_different_year_includes_year() {
        let past_year = Local::now().year() - 1;
        let dt = Local
            .with_ymd_and_hms(past_year, 8, 9, 4, 1, 0)
            .single()
            .unwrap();
        let ts = dt.timestamp();

        let formatted = format_datetime_prose(ts, &TimeFormat::Human);

        assert_eq!(formatted, format!("4:01 Sat 9 Aug {}", past_year));
    }

    #[test]
    fn test_missed_notification_uses_prose_formatting() {
        let current_year = Local::now().year();
        let dt = Local
            .with_ymd_and_hms(current_year, 8, 9, 4, 1, 0)
            .single()
            .unwrap();
        let missed = vec![Reminder {
            id: ReminderId::History(1),
            message: "Single digit test".to_string(),
            trigger_at: dt.timestamp(),
            status: Status::Missed,
        }];

        let notifications = build_missed_notifications(&missed, &TimeFormat::Human);
        assert_eq!(notifications.len(), 1);

        assert!(
            notifications[0].body.starts_with("4:01 Sun 9 Aug"),
            "Expected body to start with trimmed datetime, got: {}",
            notifications[0].body
        );
    }

    #[test]
    fn test_single_missed_notification_iso() {
        let missed = vec![Reminder {
            id: ReminderId::History(1),
            message: "Buy milk".to_string(),
            trigger_at: 1700000000,
            status: Status::Missed,
        }];

        let expected_date = chrono::DateTime::from_timestamp(missed[0].trigger_at, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();

        let notifications = build_missed_notifications(&missed, &TimeFormat::Iso);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].summary, "Missed Reminder");
        assert!(notifications[0].body.starts_with(&expected_date));
        assert!(notifications[0].body.contains("Buy milk"));
    }

    #[test]
    fn test_bulk_missed_notifications() {
        let missed = vec![
            Reminder {
                id: ReminderId::History(1),
                message: "Task 1".into(),
                trigger_at: 100,
                status: Status::Missed,
            },
            Reminder {
                id: ReminderId::History(2),
                message: "Task 2".into(),
                trigger_at: 100,
                status: Status::Missed,
            },
            Reminder {
                id: ReminderId::History(3),
                message: "Task 3".into(),
                trigger_at: 100,
                status: Status::Missed,
            },
        ];

        let notifications = build_missed_notifications(&missed, &TimeFormat::Human);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].summary, "Missed Reminders");
        assert_eq!(
            notifications[0].body,
            "3 missed notifications.\nRun 'rmd ls' for details."
        );
    }
}
