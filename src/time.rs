use anyhow::{Result, anyhow};
use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
    Weekday,
};

// Time constants matching format_time_left
const SEC_PER_MIN: i64 = 60;
const SEC_PER_HOUR: i64 = 3600;
const SEC_PER_DAY: i64 = 86400;
const SEC_PER_WEEK: i64 = 604800; // 7 days
const SEC_PER_MONTH: i64 = 2_592_000; // 30 days
const SEC_PER_YEAR: i64 = 31_536_000; // 365 days

const KEYWORDS: [&str; 16] = [
    "wednesday",
    "thursday",
    "saturday",
    "tomorrow",
    "tuesday",
    "monday",
    "friday",
    "sunday",
    "today",
    "wed",
    "thu",
    "sat",
    "tue",
    "mon",
    "fri",
    "sun",
];

/// Parses a time specifier string into a Unix timestamp in seconds.
pub fn parse_time(input: &str) -> Result<i64> {
    let now = Local::now();
    parse_time_relative_to(input, now)
}

/// Core parsing logic with an explicit reference time (useful for deterministic unit testing).
pub fn parse_time_relative_to(input: &str, now: DateTime<Local>) -> Result<i64> {
    let s = input.trim();
    if s.is_empty() {
        return Err(anyhow!("Empty time specification"));
    }

    // 1. Try parsing as relative duration (+10m, 10m, 1h 30m, 2w, 1y 2mo, etc.)
    if let Ok(secs) = parse_relative_duration(s) {
        if secs <= 0 {
            return Err(anyhow!("Relative duration must be greater than zero"));
        }
        return Ok(now.timestamp() + secs);
    }

    // 2. Try parsing as absolute or keyword date/time
    let target_dt = parse_absolute_or_keyword(s, now)?;
    let target_ts = target_dt.timestamp();

    // Reject past timestamps with explicit error message
    if target_ts <= now.timestamp() {
        let formatted_target = target_dt.format("%Y-%m-%d %H:%M:%S");
        let formatted_now = now.format("%Y-%m-%d %H:%M:%S");
        return Err(anyhow!(
            "Target time is in the past: {} (current time: {})",
            formatted_target,
            formatted_now
        ));
    }

    Ok(target_ts)
}

/// Parses relative duration specifications.
/// Supports both leading '+' and plain strings like "10m", "2h30m", "3w", "2mo", "1y".
fn parse_relative_duration(input: &str) -> Result<i64> {
    let mut s = input.trim();
    if let Some(stripped) = s.strip_prefix('+') {
        s = stripped.trim();
    }

    if s.is_empty() {
        return Err(anyhow!("Invalid relative duration"));
    }

    let mut total_secs: i64 = 0;
    let mut matched_any = false;
    let mut rest = s;

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }

        // Extract numeric prefix
        let digits_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len == 0 {
            return Err(anyhow!("Invalid character in duration: '{}'", rest));
        }

        let num: i64 = rest[..digits_len]
            .parse()
            .map_err(|_| anyhow!("Failed to parse duration number"))?;

        rest = &rest[digits_len..];

        // Match unit suffix (checking 'mo' before 'm' to avoid ambiguity)
        let (unit_secs, unit_len) = if rest.starts_with("mo") {
            (SEC_PER_MONTH, 2)
        } else if rest.starts_with('y') {
            (SEC_PER_YEAR, 1)
        } else if rest.starts_with('w') {
            (SEC_PER_WEEK, 1)
        } else if rest.starts_with('d') {
            (SEC_PER_DAY, 1)
        } else if rest.starts_with('h') {
            (SEC_PER_HOUR, 1)
        } else if rest.starts_with('m') {
            (SEC_PER_MIN, 1)
        } else if rest.starts_with('s') {
            (1, 1)
        } else {
            return Err(anyhow!("Unknown time unit in duration: '{}'", rest));
        };

        total_secs = total_secs
            .checked_add(
                num.checked_mul(unit_secs)
                    .ok_or_else(|| anyhow!("Duration overflow"))?,
            )
            .ok_or_else(|| anyhow!("Duration overflow"))?;

        rest = &rest[unit_len..];
        matched_any = true;
    }

    if matched_any && rest.is_empty() {
        Ok(total_secs)
    } else {
        Err(anyhow!("Failed to parse relative duration"))
    }
}

// --- Absolute / Keyword Parsers (Refactored Chain of Responsibility) ---

/// Main dispatcher for absolute date/time parsing.
fn parse_absolute_or_keyword(input: &str, now: DateTime<Local>) -> Result<DateTime<Local>> {
    let s = input.trim();

    try_parse_iso(s, &now)
        .or_else(|| try_parse_date_only(s, &now))
        .or_else(|| try_parse_time_only(s, &now))
        .or_else(|| try_parse_keyword_or_weekday(s, &now))
        .ok_or_else(|| anyhow!("Invalid date/time specifier: '{}'", input))
}

/// Parses full ISO/Compound Datetime: "YYYY-MM-DD HH:MM:SS", "YYYY-MM-DDTHH:MM", "YYYY-MM-DD@HH:MM"
fn try_parse_iso(s: &str, now: &DateTime<Local>) -> Option<DateTime<Local>> {
    let s_normalized = if s.len() >= 11 && matches!(s.as_bytes()[10], b'T' | b't' | b'@') {
        let mut string = s.to_string();
        string.replace_range(10..11, " ");
        string
    } else {
        s.to_string()
    };

    NaiveDateTime::parse_from_str(&s_normalized, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(&s_normalized, "%Y-%m-%d %H:%M"))
        .ok()
        .and_then(|ndt| naive_to_local(&ndt, now).ok())
}

/// Parses date-only inputs: "YYYY-MM-DD" (defaults to midnight 00:00:00).
fn try_parse_date_only(s: &str, now: &DateTime<Local>) -> Option<DateTime<Local>> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|nd| nd.and_hms_opt(0, 0, 0))
        .and_then(|ndt| naive_to_local(&ndt, now).ok())
}

/// Parses time-only inputs: "HH:MM" or "HH:MM:SS".
/// Rolls over to tomorrow if the time has already passed today.
fn try_parse_time_only(s: &str, now: &DateTime<Local>) -> Option<DateTime<Local>> {
    let nt = parse_raw_time(s).ok()?;
    let today_date = now.date_naive();
    let today_ndt = today_date.and_hms_opt(nt.hour(), nt.minute(), nt.second())?;

    if let Ok(today_dt) = naive_to_local(&today_ndt, now) {
        if today_dt > *now {
            return Some(today_dt);
        }
    }

    // Time has passed today -> assume tomorrow at the same time
    let tomorrow_date = today_date.succ_opt()?;
    let tomorrow_ndt = tomorrow_date.and_hms_opt(nt.hour(), nt.minute(), nt.second())?;
    naive_to_local(&tomorrow_ndt, now).ok()
}

/// Parses keyword or weekday combinations (e.g., "tomorrow 15:00", "mon09:00", "friday@18:30").
fn try_parse_keyword_or_weekday(s: &str, now: &DateTime<Local>) -> Option<DateTime<Local>> {
    let s_lower = s.to_lowercase();

    // Keyword / Weekday + Time combinations
    // Supports:
    // - Spaced: "tomorrow 15:00", "mon 09:00"
    // - Concatenated: "tomorrow15:00", "mon09:00", "friday18:30"
    // - Separated by '@': "tomorrow@15:00", "mon@09:00"
    for &kw in &KEYWORDS {
        if s_lower.starts_with(kw) {
            let rest = &s[kw.len()..];
            let rest_clean =
                rest.trim_start_matches(|c: char| c == '@' || c == ':' || c.is_whitespace());

            let nt = parse_raw_time(rest_clean).ok()?;
            let target_date = resolve_keyword_date(kw, nt, now)?;
            let ndt = target_date.and_hms_opt(nt.hour(), nt.minute(), nt.second())?;

            return naive_to_local(&ndt, now).ok();
        }
    }

    None
}

/// Resolves the target target NaiveDate for keywords ("today", "tomorrow") or weekdays ("mon", "friday").
fn resolve_keyword_date(kw: &str, time: NaiveTime, now: &DateTime<Local>) -> Option<NaiveDate> {
    match kw {
        "today" => Some(now.date_naive()),
        "tomorrow" => now.date_naive().succ_opt(),
        _ => {
            let weekday = parse_weekday(kw).ok()?;
            let current_weekday = now.weekday();
            let mut days_ahead =
                (weekday.num_days_from_monday() + 7 - current_weekday.num_days_from_monday()) % 7;

            if days_ahead == 0 {
                // Same weekday: check if time has already passed today
                if let Some(ndt) =
                    now.date_naive()
                        .and_hms_opt(time.hour(), time.minute(), time.second())
                {
                    if let Ok(dt) = naive_to_local(&ndt, now) {
                        if dt <= *now {
                            days_ahead = 7; // Target next week's day
                        }
                    }
                }
            }
            now.date_naive()
                .checked_add_signed(Duration::days(days_ahead as i64))
        }
    }
}

// --- Helpers ---

fn parse_raw_time(s: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M"))
        .map_err(|_| anyhow!("Invalid time format"))
}

fn parse_weekday(s: &str) -> Result<Weekday> {
    match s {
        "mon" | "monday" => Ok(Weekday::Mon),
        "tue" | "tuesday" => Ok(Weekday::Tue),
        "wed" | "wednesday" => Ok(Weekday::Wed),
        "thu" | "thursday" => Ok(Weekday::Thu),
        "fri" | "friday" => Ok(Weekday::Fri),
        "sat" | "saturday" => Ok(Weekday::Sat),
        "sun" | "sunday" => Ok(Weekday::Sun),
        _ => Err(anyhow!("Invalid weekday")),
    }
}

fn naive_to_local(ndt: &NaiveDateTime, now: &DateTime<Local>) -> Result<DateTime<Local>> {
    now.timezone()
        .from_local_datetime(ndt)
        .single()
        .ok_or_else(|| anyhow!("Ambiguous or invalid local date/time"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ref_time() -> DateTime<Local> {
        // Reference point: Sunday, 2026-08-09 12:00:00
        Local
            .with_ymd_and_hms(2026, 8, 9, 12, 0, 0)
            .single()
            .expect("Valid local reference time")
    }

    #[test]
    fn test_relative_durations() {
        let now = ref_time();

        assert_eq!(
            parse_time_relative_to("+10m", now).unwrap(),
            now.timestamp() + 600
        );
        assert_eq!(
            parse_time_relative_to("10m", now).unwrap(),
            now.timestamp() + 600
        );
        assert_eq!(
            parse_time_relative_to("2h 30m", now).unwrap(),
            now.timestamp() + 9000
        );
        assert_eq!(
            parse_time_relative_to("1w", now).unwrap(),
            now.timestamp() + SEC_PER_WEEK
        );
        assert_eq!(
            parse_time_relative_to("2mo", now).unwrap(),
            now.timestamp() + 2 * SEC_PER_MONTH
        );
        assert_eq!(
            parse_time_relative_to("1y", now).unwrap(),
            now.timestamp() + SEC_PER_YEAR
        );
    }

    #[test]
    fn test_time_only() {
        let now = ref_time(); // 2026-08-09 12:00:00

        // Future time today
        let t1 = parse_time_relative_to("15:30", now).unwrap();
        let dt1 = Local.timestamp_opt(t1, 0).unwrap();
        assert_eq!(
            dt1.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-08-09 15:30:00"
        );

        // Past time today -> rolls over to tomorrow
        let t2 = parse_time_relative_to("09:00", now).unwrap();
        let dt2 = Local.timestamp_opt(t2, 0).unwrap();
        assert_eq!(
            dt2.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-08-10 09:00:00"
        );
    }

    #[test]
    fn test_keywords_and_concatenated() {
        let now = ref_time(); // 2026-08-09 12:00:00 (Sunday)

        // Spaced
        let t_tom = parse_time_relative_to("tomorrow 15:00", now).unwrap();
        assert_eq!(
            Local
                .timestamp_opt(t_tom, 0)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-10 15:00"
        );

        // Concatenated
        let t_tom_cat = parse_time_relative_to("tomorrow15:00", now).unwrap();
        assert_eq!(t_tom, t_tom_cat);

        // @-separated
        let t_tom_at = parse_time_relative_to("tomorrow@15:00", now).unwrap();
        assert_eq!(t_tom, t_tom_at);

        // Weekdays concatenated
        let t_mon = parse_time_relative_to("mon09:00", now).unwrap();
        assert_eq!(
            Local
                .timestamp_opt(t_mon, 0)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-10 09:00"
        );

        let t_fri = parse_time_relative_to("friday18:30", now).unwrap();
        assert_eq!(
            Local
                .timestamp_opt(t_fri, 0)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-14 18:30"
        );

        // Spaced relative time
        let t_multi = parse_time_relative_to("2h 30m", now).unwrap();
        assert_eq!(t_multi, now.timestamp() + 9000);
    }

    #[test]
    fn test_iso_and_full_datetime() {
        let now = ref_time();

        // Standard space-separated
        let t1 = parse_time_relative_to("2026-08-15 14:00", now).unwrap();
        assert_eq!(
            Local
                .timestamp_opt(t1, 0)
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-15 14:00"
        );

        // ISO with 'T', 't', and '@' separator
        let t2 = parse_time_relative_to("2026-08-15T14:00", now).unwrap();
        let t3 = parse_time_relative_to("2026-08-15t14:00:00", now).unwrap();
        let t4 = parse_time_relative_to("2026-08-15@14:00", now).unwrap();

        assert_eq!(t1, t2);
        assert_eq!(t1, t3);
        assert_eq!(t1, t4);
    }

    #[test]
    fn test_past_time_rejection() {
        let now = ref_time(); // 2026-08-09 12:00:00

        let res = parse_time_relative_to("today 09:00", now);
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Target time is in the past")
        );
    }
}
