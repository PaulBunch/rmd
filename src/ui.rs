use terminal_size::{Width, terminal_size};

// Import Reminder struct from main module if it's defined in main.rs
use crate::Reminder;

pub fn print_reminders_table(reminders: &[Reminder]) {
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

    // Format timestamps and prepare string rows
    let rows: Vec<(String, String, String)> = reminders
        .iter()
        .map(|r| {
            let naive = chrono::DateTime::from_timestamp(r.trigger_at, 0).unwrap_or_default();
            let time_str = naive.format("%Y-%m-%d %H:%M").to_string();
            (r.id.to_string(), time_str, r.message.clone())
        })
        .collect();

    // 5. Dynamic column width calculation based on content vs header lengths
    let id_width = rows
        .iter()
        .map(|(id, _, _)| id.len())
        .max()
        .unwrap_or(0)
        .max("ID".len());

    let time_width = rows
        .iter()
        .map(|(_, time, _)| time.len())
        .max()
        .unwrap_or(0)
        .max("TIME".len());

    let gap = "  "; // 2 spaces gap between columns
    let prefix_len = id_width + gap.len() + time_width + gap.len();

    // Calculate maximum available space for MESSAGE column
    let max_msg_avail = if term_width > prefix_len + 4 {
        term_width - prefix_len
    } else {
        "MESSAGE".len()
    };

    let msg_width = rows
        .iter()
        .map(|(_, _, msg)| msg.chars().count())
        .max()
        .unwrap_or(0)
        .max("MESSAGE".len())
        .min(max_msg_avail);

    // 2. Format headers with ANSI underline on the same line
    let header_id = format!("{:<width$}", "ID", width = id_width);
    let header_time = format!("{:<width$}", "TIME", width = time_width);
    let header_msg = format!("{:<width$}", "MESSAGE", width = msg_width);

    println!(
        "\x1b[4m{}\x1b[0m{}\x1b[4m{}\x1b[0m{}\x1b[4m{}\x1b[0m",
        header_id, gap, header_time, gap, header_msg
    );

    // Print data rows with message truncation if exceeding width
    for (id, time, msg) in rows {
        // 6. Truncate long messages with '...'
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
            "{:<id_w$}{}{:<time_w$}{}{}",
            id,
            gap,
            time,
            gap,
            truncated_msg,
            id_w = id_width,
            time_w = time_width
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
