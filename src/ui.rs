use chrono::{Datelike, Local};
use terminal_size::{Width, terminal_size};

use crate::Reminder;
use crate::TimeFormat;

/// Helper to format a timestamp based on TimeFormat for inline CLI messages
pub fn format_datetime(timestamp: i64, time_format: &TimeFormat) -> String {
    let local_dt = chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.with_timezone(&Local))
        .unwrap_or_default();

    let current_year = Local::now().year();

    match time_format {
        TimeFormat::Iso => local_dt.format("%Y-%m-%d %H:%M").to_string(),
        TimeFormat::Human => {
            let raw = if local_dt.year() == current_year {
                local_dt.format("%k:%M %a %e %b").to_string()
            } else {
                local_dt.format("%k:%M %a %e %b %Y").to_string()
            };
            // Clean up extra alignment padding for prose output
            raw.split_whitespace().collect::<Vec<_>>().join(" ")
        }
    }
}

pub fn format_add_response(reminder: &Reminder, time_format: &TimeFormat) -> String {
    let time_str = format_datetime(reminder.trigger_at, time_format);
    let now = Local::now().timestamp();
    let diff = reminder.trigger_at.saturating_sub(now);
    let left_str = format_time_left(if diff > 0 { diff as u64 } else { 0 });

    format!(
        "Added reminder {} in {}: {} — {}",
        reminder.id, left_str, time_str, reminder.message
    )
}

pub fn format_remove_response(reminder: &Reminder, time_format: &TimeFormat) -> String {
    let time_str = format_datetime(reminder.trigger_at, time_format);
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

pub fn print_reminders_table(reminders: &[Reminder], time_format: &TimeFormat) {
    let count = reminders.len();

    // 1. Blank line before table
    println!();

    if reminders.is_empty() {
        println!("No active reminders");
        println!();
        println!("0 reminders");
        return;
    }

    // Get terminal width (default to 80 if not running in a TTY)
    let term_width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    let now_dt = Local::now();
    let current_year = now_dt.year();

    // Format timestamps and prepare string rows in OS local time
    let rows: Vec<(String, String, String, String)> = reminders
        .iter()
        .map(|r| {
            let local_dt = chrono::DateTime::from_timestamp(r.trigger_at, 0)
                .map(|dt| dt.with_timezone(&Local))
                .unwrap_or_default();

            let time_str = match time_format {
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
                        local_dt.format("%k:%M %a %-d %b %Y").to_string()
                    }
                }
            };

            let diff = r.trigger_at.saturating_sub(now_dt.timestamp());
            let left_str = format_time_left(if diff > 0 { diff as u64 } else { 0 });

            (r.id.to_string(), time_str, left_str, r.message.clone())
        })
        .collect();

    // Dynamic column width calculation based on content vs header lengths
    let id_width = rows
        .iter()
        .map(|(id, _, _, _)| id.len())
        .max()
        .unwrap_or(0)
        .max("ID".len());

    let time_width = rows
        .iter()
        .map(|(_, time, _, _)| time.len())
        .max()
        .unwrap_or(0)
        .max("TIME".len());

    let left_width = rows
        .iter()
        .map(|(_, _, left, _)| left.len())
        .max()
        .unwrap_or(0)
        .max("LEFT".len());

    let gap = "  "; // 2 spaces gap between columns
    let prefix_len = id_width + gap.len() + time_width + gap.len() + left_width + gap.len();

    // Calculate maximum available space for MESSAGE column
    let max_msg_avail = if term_width > prefix_len + 4 {
        term_width - prefix_len
    } else {
        "MESSAGE".len()
    };

    let msg_width = rows
        .iter()
        .map(|(_, _, _, msg)| msg.chars().count())
        .max()
        .unwrap_or(0)
        .max("MESSAGE".len())
        .min(max_msg_avail);

    // 2. Format headers with ANSI underline on the same line
    let header_id = format!("{:<width$}", "ID", width = id_width);
    let header_time = format!("{:<width$}", "TIME", width = time_width);
    let header_left = format!("{:<width$}", "LEFT", width = left_width);
    let header_msg = format!("{:<width$}", "MESSAGE", width = msg_width);

    println!(
        "\x1b[4m{}\x1b[0m{}\x1b[4m{}\x1b[0m{}\x1b[4m{}\x1b[0m{}\x1b[4m{}\x1b[0m",
        header_id, gap, header_time, gap, header_left, gap, header_msg
    );

    // Print data rows with message truncation if exceeding width
    for (id, time, left, msg) in rows {
        // Truncate long messages with '...'
        let truncated_msg = if msg.chars().count() > msg_width {
            if msg_width > 3 {
                let mut s: String = msg.chars().take(msg_width - 3).collect();
                s.push_str("...");
                s
            } else {
                msg.chars().take(msg_width).collect()
            }
        } else {
            msg
        };

        println!(
            "{:<id_w$}{}{:<time_w$}{}{:<left_w$}{}{}",
            id,
            gap,
            time,
            gap,
            left,
            gap,
            truncated_msg,
            id_w = id_width,
            time_w = time_width,
            left_w = left_width
        );
    }

    // 3. Blank line after table
    println!();

    // 4. Summary output
    if count == 1 {
        println!("1 reminder");
    } else {
        println!("{} reminders", count);
    }
}
